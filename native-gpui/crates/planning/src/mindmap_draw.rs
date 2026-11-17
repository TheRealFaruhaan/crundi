//! The Mind map's screen (the Slint client's ui/planning/mindmap.slint and
//! mindmap-dialogs.slint, redrawn in GPUI): the toolbar (+ Add idea, Media,
//! Filter ideas…, Shortcuts), the project bar, the canvas (the ideas laid out
//! as a tree, the links, the "add idea" boxes, the box an idea is typed in,
//! the ring the keyboard moves, a drag's clone) and the dialogs (an idea's
//! details, the link picker, the shortcut sheet). Zoom scales every size on
//! the canvas by M.zoom (GPUI has no transforms: the web scales one layer).
//! Everything on the canvas scales with it, so its parts are drawn here.

use super::{help, tree, Key, Row, M};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::input::{Enter, Escape, IndentInline, OutdentInline, SelectToNextWordEnd, SelectToPreviousWordStart};
use std::rc::Rc;

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
fn meta_chip(id: String, kind: &str, text: &str, todo: i32, tip: &str, z: f32) -> AnyElement {
    let fg = match kind { "link" => T::green(), "scope" => T::accent_hover(), "note" => T::text_secondary(), _ => T::text_muted() };
    let bg = match kind { "link" => T::green_dim(), "scope" => T::accent_dim(), _ => T::bg_tertiary() };
    let icon = match kind { "link" | "missing" => Some(IconName::Link), "scope" => Some(IconName::Folder), "note" => Some(IconName::StickyNote), _ => None };
    let d = div().id(SharedString::from(id)).flex().flex_none().items_center().gap(T::SP_2 * z).px(T::SP_6 * z).py(px(1.) * z).rounded(T::R_FIELD * z)
        .bg(bg).text_color(fg).text_size(T::TEXT_11 * z).max_w(T::MINDMAP_NODE * z - T::SP_24 * z)
        .when(kind == "inherited", |d| d.border_1().border_dashed().border_color(T::text_muted()))
        .when_some(icon, |d, i| d.child(w::icon(i, T::ICON_SM * z)))
        .when(todo > 0, |d| d.child(w::icon(if todo == 2 { IconName::SquareCheck } else { IconName::Square }, T::ICON_SM * z)))
        .child(div().min_w_0().truncate().child(text.to_string()));
    w::tip(d, tip.to_string()).into_any_element()
}

/// An idea (.mm-node): its words (two lines; Open: all of it, about seven
/// lines, then it scrolls; Editing: a text box in place), its chips, the − / +
/// that folds what is under it.
fn card(r: &Row, mode: Mode, z: f32, edit: Option<&Entity<w::TextareaState>>) -> AnyElement {
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
        .when(!r.chip.is_empty(), |d| d.child(meta_chip(format!("mm-chip-{tag}-{}", r.id), &r.chip, &r.chip_text, r.todo, &r.chip_tip, z)))
        .when(r.notes > 0, |d| d.child(meta_chip(format!("mm-note-{tag}-{}", r.id), "note", &format!("{} {}", r.notes, if r.notes == 1 { "note" } else { "notes" }), 0, "", z))));
    let id = r.id.clone();
    let mut d = div().id(SharedString::from(format!("mm-{tag}-{}", r.id))).absolute().left(px(r.x * z)).top(px(top)).w(T::MINDMAP_NODE * z)
        .flex().flex_col().gap(T::SP_6 * z).px(T::SP_10 * z + px(1.)).py(T::SP_8 * z).rounded(T::R_BUBBLE * z)
        .bg(if r.root && !on { T::accent_dim() } else { T::bg_card() })
        .when(strong, |d| d.border_2()).when(!strong, |d| d.border_1()).border_color(border)
        .when(!strong && !r.linked && !r.root && !r.scoped, |d| d.hover(|d| d.border_color(T::accent())))
        .when(r.pending, |d| d.border_dashed().border_color(T::text_muted()))
        .when(r.lifted || r.dim, |d| d.opacity(T::MINDMAP_O_FAINT))
        .when(r.flash || r.target, |d| d.shadow(glow())).when(on && !(r.flash || r.target), |d| d.shadow(lift()))
        // A root opened up keeps its tint over the card it covers.
        .when(r.root && on, |d| d.child(div().absolute().top_0().left_0().size_full().rounded(T::R_BUBBLE * z).bg(T::accent_dim())))
        .child(body)
        .children(chips);
    if mode == Mode::Editing {
        d = d.cursor_text().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    } else {
        let (i1, i2) = (id.clone(), id.clone());
        let row = r.clone();
        d = d.cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |e, _, cx| { cx.stop_propagation(); super::node_down(&i1, f32::from(e.position.x), f32::from(e.position.y), cx); })
            .on_mouse_up(MouseButton::Left, move |e, window, cx| { cx.stop_propagation(); super::node_up(&i2, e.click_count, window, cx); })
            .on_hover(move |hot, window, cx| {
                if *hot { if mode == Mode::Normal { let c = clamped(&row.text, z, window); super::hover(&row, true, c, cx); } }
                // The idea opened up decides when it closes (it lies over this one).
                else if mode == Mode::Open || cx.try_global::<M>().is_some_and(|s| s.hover.as_ref().is_none_or(|h| h.id != row.id)) { super::hover(&row, false, false, cx); }
            });
        if mode == Mode::Normal { d = d.child(measure(id.clone(), z)); }
    }
    // Folds or unfolds what is under it (.mm-collapse), on its right edge.
    if r.kids && mode != Mode::Editing {
        let fid = id.clone();
        d = d.child(div().absolute().top_0().bottom_0().left(T::MINDMAP_NODE * z - T::SP_10 * z).flex().items_center()
            .child(div().id(SharedString::from(format!("mm-fold-{tag}-{id}"))).size(T::SP_16 * z + T::SP_2 * z).rounded_full().border_1().border_color(T::border())
                .hover(|d| d.border_color(T::accent())).bg(T::bg_tertiary()).flex().items_center().justify_center().cursor_pointer()
                .text_color(T::text_secondary()).child(w::icon(if r.collapsed { IconName::Plus } else { IconName::Minus }, T::ICON_SM * z))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, window, cx| super::act("toggle", &fid, window, cx))));
    }
    d.into_any_element()
}

/// The dashed "add idea" box under an idea (.mm-ghost): half seen until hovered.
fn ghost(r: &Row, z: f32) -> AnyElement {
    let parent = r.parent.clone();
    div().id(SharedString::from(format!("mm-g-{}", r.id))).absolute().left(px(r.x * z)).top(px(r.y * z - r.h * z / 2.)).w(T::MINDMAP_NODE * z).min_h(T::SP_40 * z)
        .flex().items_center().justify_center().gap(T::SP_6 * z).py(T::SP_8 * z).rounded(T::R_BUBBLE * z)
        .border_1().border_dashed().border_color(T::border()).opacity(T::MINDMAP_O_DIM).text_color(T::text_muted()).cursor_pointer()
        .hover(|d| d.opacity(1.).border_color(T::accent()).text_color(T::accent_hover()))
        .child(w::icon(IconName::Plus, T::ICON_MD * z))
        .child(div().text_size(T::TEXT_12 * z).font_weight(T::W_MEDIUM).child("add idea"))
        .child(measure(r.id.clone(), z))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, cx| super::act("ghost", &parent, window, cx))
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

/// The idea in hand while dragging (.drag-clone).
fn clone_box(s: &M, z: f32) -> AnyElement {
    div().absolute().left(px((s.drag_x - s.grab_x) * z)).top(px((s.drag_y - s.grab_y) * z)).w(T::MINDMAP_NODE * z)
        .px(T::SP_10 * z).py(T::SP_8 * z).rounded(T::R_BUBBLE * z).border_1().border_color(T::accent()).bg(T::bg_card())
        .opacity(T::MINDMAP_O_CLONE).shadow(lift())
        .child(div().text_size(T::TEXT_13 * z).font_weight(T::W_SEMIBOLD).text_color(T::text()).line_clamp(2).text_ellipsis().child(s.drag_text.clone()))
        .into_any_element()
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

/// The canvas (.mindmap-canvas): dotted, scrolls both ways; Ctrl and the
/// wheel zoom about the pointer.
fn canvas_view(s: &M, window: &mut Window) -> AnyElement {
    let _ = window;
    let z = s.zoom;
    let (cw, ch) = (s.view_w.max(s.base_w * z), s.view_h.max(s.base_h * z));
    let (solid, dashed) = (s.lay.curves.clone(), s.lay.dash_curves.clone());
    let edges = canvas(|_, _, _| {}, move |b, _, window, _| {
        if !solid.is_empty() {
            let mut pb = PathBuilder::stroke(T::SP_2 * z);
            curves(&mut pb, &solid, b.origin, z);
            if let Ok(p) = pb.build() { window.paint_path(p, T::border()); }
        }
        if !dashed.is_empty() {
            let dash = (T::SP_4 + px(1.)) * z;
            let mut pb = PathBuilder::stroke(T::SP_2 * z).dash_array(&[dash, dash]);
            curves(&mut pb, &dashed, b.origin, z);
            if let Ok(p) = pb.build() { window.paint_path(p, T::border().opacity(T::MINDMAP_O_DIM)); }
        }
    }).absolute().top_0().left_0().w(px(s.base_w * z)).h(px(s.base_h * z));
    let mut content = div().id("mm-content").relative().w(px(cw)).h(px(ch))
        // The bare canvas: a press lets go of the ring and closes a box.
        .on_mouse_down(MouseButton::Left, |_, window, cx| super::canvas_press(window, cx))
        .on_scroll(|e: &ScrollEvent, window, cx| {
            if !e.modifiers.control { return; }
            let dy = f32::from(e.delta.pixel_delta(T::TEXT_16).y);
            let (ox, oy) = cx.try_global::<M>().map(|s| (s.canvas_x, s.canvas_y)).unwrap_or_default();
            super::wheel_zoom(dy, f32::from(e.position.x) - ox, f32::from(e.position.y) - oy, window, cx);
            cx.stop_propagation();
        })
        // A touchpad's pinch zooms about its centre, as Ctrl and the wheel do
        // (the help promised it; the owner, 18 Nov: no pinch to zoom).
        .on_pinch(|e: &PinchEvent, window, cx| {
            let (ox, oy) = cx.try_global::<M>().map(|s| (s.canvas_x, s.canvas_y)).unwrap_or_default();
            super::pinch_zoom(e.delta, f32::from(e.position.x) - ox, f32::from(e.position.y) - oy, window, cx);
            cx.stop_propagation();
        })
        .child(edges);
    for r in &s.rows {
        match r.kind {
            "node" => content = content.child(card(r, Mode::Normal, z, None)),
            "ghost" if !r.hidden => content = content.child(ghost(r, z)),
            "drop" => content = content.child(drop_box(r, z)),
            _ => {}
        }
    }
    let ui = s.ui.as_ref();
    if s.ring_on { content = content.child(ring(s, z)); }
    if let Some(h) = &s.hover { if !(s.edit_on && s.edit_node.as_ref().is_some_and(|e| e.id == h.id)) { content = content.child(card(h, Mode::Open, z, None)); } }
    if s.edit_on { if let Some(e) = &s.edit_node { content = content.child(card(e, Mode::Editing, z, ui.map(|u| &u.edit))); } }
    if s.draft_on { if let Some(u) = ui { content = content.child(draft(s, z, &u.draft)); } }
    if !s.drag_id.is_empty() { content = content.child(clone_box(s, z)); }
    div().relative().flex_1().min_h_0().overflow_hidden()
        .child(canvas(|b, _, cx| super::canvas_measured(b, cx), |b, _, window, _| {
            let step = f32::from(T::MINDMAP_DOTS);
            let (w_, h_) = (f32::from(b.size.width), f32::from(b.size.height));
            let mut y = 0.;
            while y < h_ {
                let mut x = 0.;
                while x < w_ { window.paint_quad(fill(Bounds::new(point(b.origin.x + px(x), b.origin.y + px(y)), size(T::SP_2, T::SP_2)), T::border_subtle()).corner_radii(px(1.))); x += step; }
                y += step;
            }
        }).absolute().top_0().left_0().size_full())
        .child(div().id("mm-scroll").absolute().top_0().left_0().size_full().overflow_scroll().track_scroll(&s.scroll).child(content))
        .into_any_element()
}

// ─── The dialogs ───

fn modal(id: &'static str, width: Pixels, focus: FocusHandle, scroll: ScrollHandle, close: impl Fn(&mut Window, &mut App) + 'static) -> w::Modal {
    let close = Rc::new(close);
    let c2 = close.clone();
    w::Modal { id, width, focus, scroll, on_close: Rc::new(move |window, cx| close(window, cx)),
        on_key: Rc::new(move |e, window, cx| if e.keystroke.key == "escape" { c2(window, cx); cx.stop_propagation(); }) }
}

fn label(t: &'static str) -> Div { div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(t) }

/// An idea's details (#mm-detail-modal).
fn detail(s: &M, window: &mut Window) -> Option<AnyElement> {
    let u = s.ui.as_ref()?;
    let m = modal("mm-detail", T::MINDMAP_DIALOG_W, u.d_focus.clone(), u.d_scroll.clone(), |window, cx| super::d_act("close", window, cx));
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
    Some(w::overlay_modal(m, window, vec![col.into_any_element()]).into_any_element())
}

/// The link picker (#link-modal): the project, a search, "Scope to …", and
/// the project's tasks with their subtasks.
fn link(s: &M, window: &mut Window) -> Option<AnyElement> {
    let u = s.ui.as_ref()?;
    let m = modal("mm-link", T::MINDMAP_DIALOG_W, u.l_focus.clone(), u.l_scroll.clone(), |window, cx| super::l_act("cancel", "", "", window, cx));
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
    Some(w::overlay_modal(m, window, vec![col.into_any_element()]).into_any_element())
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
fn help_sheet(s: &M, window: &mut Window) -> Option<AnyElement> {
    let u = s.ui.as_ref()?;
    let m = modal("mm-help", T::DIALOG_W_XL, u.h_focus.clone(), u.h_scroll.clone(), |window, cx| super::h_act("close", window, cx));
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
                        .child(div().flex_1().min_w_0().text_size(T::TEXT_13).text_color(T::text_secondary()).when(has, |d| d.hover(|d| d.text_color(T::text()))).child(text))
                        .when(has, |d| d.child(div().text_color(if open { T::accent_hover() } else { T::text_muted() })
                            .child(w::icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }, T::ICON_MD)))))
                    .when_some(dia.filter(|_| open), |d, dg| d.child(diagram(&dg)))
                    .when(has, |d| d.cursor_pointer().on_click(move |_, window, cx| { super::h_toggle(i, cx); window.refresh(); }));
                body = body.child(row);
            }
        }
    }
    Some(w::overlay_modal(m, window, vec![head.into_any_element(), body.into_any_element()]).into_any_element())
}

// ─── The screen ───

pub(super) fn screen(_key: &str, window: &mut Window, cx: &mut App) -> AnyElement {
    let s = cx.global::<M>();
    let Some(u) = s.ui.as_ref() else { return w::loading("Loading…").into_any_element() };
    let fold = s.width > 0. && s.width < f32::from(T::MINDMAP_FOLD);
    // The keys of the map (the web's document keydown while the map was the
    // last thing pressed): a press anywhere in it gives it the keys.
    let mut root = div().id("mindmap").track_focus(&u.canvas).relative().flex().flex_col().size_full().min_h_0().bg(T::bg())
        .child(canvas(|b, _, cx| { let wd = f32::from(b.size.width); if cx.has_global::<M>() { cx.global_mut::<M>().width = wd; } }, |_, _, _, _| {}).absolute().top_0().left_0().size_full())
        .key_context(super::MAP_CONTEXT)
        .on_action(|_: &super::MapTab, window, cx| { super::key(&Key { key: "tab", ctrl: false, shift: false, alt: false }, window, cx); })
        .on_action(|_: &super::MapBackTab, window, cx| { super::key(&Key { key: "tab", ctrl: false, shift: true, alt: false }, window, cx); })
        .on_key_down(|e: &KeyDownEvent, window, cx| if super::key(&key_of(&e.keystroke), window, cx) { cx.stop_propagation(); })
        .on_mouse_move(|e: &MouseMoveEvent, window, cx| super::pointer_move(f32::from(e.position.x), f32::from(e.position.y), window, cx))
        .on_mouse_up(MouseButton::Left, |_, window, cx| super::pointer_up(window, cx));
    match s.state.as_str() {
        "loading" => return root.child(w::loading("Loading…")).into_any_element(),
        "error" => return root.child(w::empty(s.error.clone())).into_any_element(),
        _ => {}
    }
    // The toolbar (.kanban-toolbar).
    root = root.child(w::toolbar()
        .child(w::primary("mm-add", "+ Add idea").on_click(|_, window, cx| super::act("add-root", "", window, cx)))
        .child(w::button("mm-media", if fold { "" } else { "Media" }).icon(IconName::Images).on_click(|_, window, cx| super::act("media", "", window, cx)))
        .child(div().w(T::SP_48 * 3. + T::SP_16).child(w::input(&u.search)))
        .child(div().flex_1())
        .child(w::button("mm-help", if fold { "?" } else { "? Shortcuts" }).on_click(|_, window, cx| super::act("help", "", window, cx))));
    if s.state == "empty" {
        root = root.child(w::empty("No ideas yet. Add one, or hit \u{201c}Brainstorm\u{201d} on a Kanban card to extend a task here."));
    } else {
        // The project bar (.mm-proj-bar): its chips wrap, as the web's.
        if s.bar {
            let mut bar = div().flex().flex_none().flex_wrap().items_center().gap(T::SP_6).px(T::SP_12).py(T::SP_8).bg(T::bg_secondary()).border_b_1().border_color(T::border())
                .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Projects:"));
            for (i, c) in s.chips.iter().enumerate() {
                let k = c.key.clone();
                bar = bar.child(w::filter_chip(("mm-proj", i), c.label.clone(), c.off, false).on_click(move |_, window, cx| super::act("chip", &k, window, cx)));
            }
            if s.hidden_count > 0 {
                bar = bar.child(div().flex().items_center().gap(T::SP_4).pl(T::SP_4).text_size(T::TEXT_12).text_color(T::amber())
                        .child(w::icon(IconName::Funnel, T::ICON_SM)).child(format!("filtered ({} hidden)", s.hidden_count)))
                    .child(w::filter_chip("mm-proj-all", "Show all", false, true).on_click(|_, window, cx| super::act("chip-all", "", window, cx)));
            }
            root = root.child(bar);
        }
        root = root.child(canvas_view(s, window));
    }
    let s = cx.global::<M>();
    let overlay = if s.d_open { detail(s, window) } else if s.l_open { link(s, window) } else if s.h_shown { help_sheet(s, window) } else { None };
    root.children(overlay).into_any_element()
}
