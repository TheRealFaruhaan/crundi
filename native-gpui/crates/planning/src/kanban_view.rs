//! Kanban's screen (the Slint client's ui/planning/kanban.slint, overlay.slint and parts.slint; the
//! web's #kanban-panel): the toolbar (+ Add task; Board, Trash (n), History, Media), the board's
//! columns with their cards, the Trash and History views, and the layers over the window (the
//! dragged card's clone, a card's attachments strip, the ask dialog). Drawn from kanban.rs's state;
//! the tab and the workbench pane (panel:kanban) draw the same.

use super::{act, board, st, with, Clip, K};
use crundi_theme::T;
use crundi_widgets::{self as w, IconName};
use gpui::{prelude::FluentBuilder as _, *};
use std::collections::HashMap;

#[derive(Default)]
struct Widths { tab: f32, pane: f32 }
impl Global for Widths {}

fn eid(p: &str, k: &str) -> ElementId { ElementId::Name(format!("{p}-{k}").into()) }

fn b4(b: Bounds<Pixels>) -> board::Box4 {
    board::Box4 { x: f32::from(b.origin.x), y: f32::from(b.origin.y), w: f32::from(b.size.width), h: f32::from(b.size.height) }
}

/// A box taken as it is laid out (for the drop target and the driver's props).
fn measure(f: impl FnOnce(board::Box4, &mut K) + 'static) -> impl IntoElement {
    canvas(move |b, _, cx| { let x = b4(b); with(cx, |k| f(x, k)); }, |_, _, _, _| {}).absolute().top_0().left_0().size_full()
}

pub fn view(embedded: bool, window: &mut Window, cx: &mut App) -> AnyElement {
    if !cx.has_global::<Widths>() { cx.set_global(Widths::default()); }
    // The boxes are taken again as this frame is laid out.
    with(cx, |k| { k.col_boxes.clear(); k.card_boxes.clear(); });
    let last = { let wd = cx.global::<Widths>(); if embedded { wd.pane } else { wd.tab } };
    let width = if last > 0. { px(last) } else { window.viewport_size().width };
    let fold = width < T::KANBAN_FOLD;
    let stacked = width < T::KANBAN_STACK;
    let k = st(cx);
    let state = k.state.clone();
    let body: AnyElement = match state.as_str() {
        "none" => muted("Select a project to use its Kanban board.").into_any_element(),
        "loading" => w::loading("Loading…").into_any_element(),
        "error" => muted(k.error.clone()).into_any_element(),
        _ => div().flex().flex_col().size_full().min_h_0()
            .child(toolbar(fold, cx))
            .child(div().flex_1().min_h_0().child(match k.view.as_str() {
                "trash" => trash(cx),
                "history" => history(cx),
                _ => board_view(stacked, width, cx),
            }))
            .into_any_element(),
    };
    let held = st(cx).press.is_some();
    let mut root = div().id(if embedded { "kanban-pane" } else { "kanban-tab" }).relative().size_full().flex().flex_col().min_h_0().bg(T::bg())
        .child(canvas(move |b, _, cx| {
            let w = f32::from(b.size.width);
            let wd = cx.global_mut::<Widths>();
            let old = if embedded { std::mem::replace(&mut wd.pane, w) } else { std::mem::replace(&mut wd.tab, w) };
            if (old - w).abs() > 0.5 { super::refresh(cx); }
        }, move |_, _, window, _| {
            // While a card is held, the pointer anywhere on the window moves it (and lets go of it).
            if !held { return; }
            window.on_mouse_event(|e: &MouseMoveEvent, phase, _, cx| {
                // The test driver's moves carry no pressed button, so the held card is ours to track.
                if phase == DispatchPhase::Bubble && (e.pressed_button == Some(MouseButton::Left) || std::env::var("CRUNDI_TEST_PORT").is_ok()) { super::pointer_move(e.position, cx); }
            });
            window.on_mouse_event(|e: &MouseUpEvent, phase, _, cx| { if phase == DispatchPhase::Bubble && e.button == MouseButton::Left { super::pointer_up(cx); } });
        }).absolute().top_0().left_0().size_full())
        .child(body);
    root = root.children(drag_clone(cx)).children(clip_pop(window, cx)).children(super::ask::render(window, cx));
    root.into_any_element()
}

/// The muted centred line (.kanban-empty).
fn muted(text: impl Into<SharedString>) -> Div {
    div().w_full().p(T::SP_24).flex().justify_center().text_size(T::TEXT_13).text_color(T::text_muted()).child(text.into())
}

fn toolbar(fold: bool, cx: &App) -> impl IntoElement {
    let k = st(cx);
    let view = k.view.clone();
    let n = k.board.as_ref().map(|b| board::trash(b).0.len()).unwrap_or(0);
    let vb = |id: &'static str, label: String, on: bool| w::toggle(id, label, on).on_click(move |_, window, cx| act(id, "", "", window, cx));
    let media = if fold { w::icon_button("kanban-media", IconName::Images, "Media") } else { w::button("kanban-media", "Media").icon(IconName::Images) };
    div().flex().flex_none().items_center()
        .gap(if fold { T::SP_4 } else { T::SP_8 })
        .px(if fold { T::SP_8 } else { T::SP_12 }).py(if fold { T::SP_6 } else { T::SP_10 })
        .border_b_1().border_color(T::border())
        .child(w::primary("kanban-add", "+ Add task").on_click(|_, window, cx| act("add-task", "", "", window, cx)))
        .child(div().flex_1())
        .child(vb("view-board", "Board".into(), view == "board"))
        .child(vb("view-trash", format!("Trash ({n})"), view == "trash"))
        .child(vb("view-history", "History".into(), view == "history"))
        .child(media.on_click(|_, window, cx| act("media", "", "", window, cx)))
}

// ─── The board ───

fn board_view(stacked: bool, width: Pixels, cx: &App) -> AnyElement {
    let k = st(cx);
    let Some(b) = k.board.as_ref() else { return div().into_any_element() };
    let mt: HashMap<String, usize> = k.media_task.iter().map(|(a, v)| (a.clone(), v.len())).collect();
    let mtd: HashMap<String, usize> = k.media_todo.iter().map(|(a, v)| (a.clone(), v.len())).collect();
    let ui = board::Ui { menu_open: Some(&k.menu_open), adding_task: k.adding_task.as_deref(), adding_todo: k.adding_todo.as_deref(), media_task: Some(&mt), media_todo: Some(&mtd) };
    let cols = board::columns(b, &ui);
    let n = cols.len().max(1) as f32;
    let pad = f32::from(T::SP_12);
    let col_w = px(((f32::from(width) - 2. * pad - (n - 1.) * pad) / n).clamp(f32::from(T::KANBAN_COL_MIN), f32::from(T::KANBAN_COL_MAX)));
    let card_w = if stacked { width - T::SP_12 * 2. - T::SP_8 * 2. } else { col_w - T::SP_8 * 2. };
    let list: Vec<AnyElement> = cols.into_iter().enumerate().map(|(i, c)| column(i, c, stacked, col_w, card_w, cx)).collect();
    if stacked {
        div().id("kanban-stack").size_full().overflow_y_scroll().p(T::SP_12).flex().flex_col().gap(T::SP_10).children(list).into_any_element()
    } else {
        div().id("kanban-board").size_full().overflow_x_scroll().p(T::SP_12).flex().items_start().gap(T::SP_12).children(list).into_any_element()
    }
}

fn column(i: usize, c: board::Col, stacked: bool, col_w: Pixels, card_w: Pixels, cx: &App) -> AnyElement {
    let k = st(cx);
    let drop_here = k.drag.is_some() && !c.other && k.target.as_ref().is_some_and(|t| t.0 == c.status);
    let before = k.target.as_ref().map(|t| t.2.clone()).unwrap_or_default();
    let line = || div().flex_none().h(T::SP_2).rounded(T::R_PILL).bg(T::accent());
    let status = c.status.clone();
    let mut body = div().flex().flex_col().p(T::SP_8).gap(T::SP_8);
    for cd in c.cards.iter() {
        if drop_here && before == cd.id { body = body.child(line()); }
        body = body.child(card(cd.clone(), &c.status, card_w, cx));
    }
    if drop_here && before.is_empty() { body = body.child(line()); }
    if !c.other { body = body.child(add_task(&c.status, c.adding, cx)); }
    let head = div().flex().flex_none().items_center().gap(T::SP_8).px(T::SP_12).pt(T::SP_10).pb(T::SP_8)
        .child(div().flex_1().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(if c.other { T::yellow() } else { T::text() }).child(c.label.clone()))
        .child(div().flex_none().min_w(T::SP_20 + T::SP_2).h(T::SP_16 + T::SP_2).px(T::SP_6).rounded(T::R_PILL).bg(T::bg_tertiary())
            .flex().items_center().justify_center().text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(c.cards.len().to_string()));
    let other = c.other;
    div().id(("kanban-col", i)).relative().flex().flex_col().min_h_0()
        .when(!stacked, |d| d.flex_none().w(col_w).max_h_full())
        // Stacked (a narrow panel), each column keeps its height in the
        // scrolling stack: shrunk to fit, its cards spilled out below and an
        // empty column all but vanished (the owner, 18 Nov).
        .when(stacked, |d| d.w_full().flex_none())
        .rounded(T::R_CARD).border_1().bg(T::bg_secondary())
        .border_color(if drop_here { T::accent() } else if other { T::yellow() } else { T::border() })
        .when(!other, |d| d.child(measure(move |b, k| k.col_boxes.push((status, b)))))
        .child(head)
        .child(w::rule())
        .child(div().id(("kanban-col-body", i)).when(!stacked, |d| d.flex_1().min_h_0().overflow_y_scroll()).child(body))
        .into_any_element()
}

/// The end of a column: "+ Add task", or the blank card it opens.
fn add_task(status: &str, adding: bool, cx: &App) -> AnyElement {
    let s = status.to_string();
    if !adding {
        return w::dashed_button(SharedString::from(format!("kanban-add-{status}")), "+ Add task")
            .on_click(move |_, window, cx| act("add-task-inline", "", &s, window, cx)).into_any_element();
    }
    let Some(i) = st(cx).task_input.clone() else { return div().into_any_element() };
    div().p(T::SP_12).rounded(T::R_CARD).border_1().border_color(T::accent_edge()).bg(T::bg_card())
        .capture_key_down(|e, window, cx| if e.keystroke.key == "escape" { super::cancel_line(false, window, cx); })
        .child(w::input(&i)).into_any_element()
}

fn card(cd: board::Card, status: &str, card_w: Pixels, cx: &App) -> AnyElement {
    let k = st(cx);
    let fold = card_w < if cd.compact { T::KANBAN_CARD_FOLD_COMPACT } else { T::KANBAN_CARD_FOLD };
    let dragged = k.drag.as_ref().is_some_and(|d| d.id == cd.id);
    let clip_open = k.clip.as_ref().is_some_and(|c| c.task == cd.id);
    let id = cd.id.clone();
    let sid = SharedString::from(cd.id.clone());
    let (st_, id2) = (status.to_string(), cd.id.clone());
    let a = |name: &'static str, id: String| move |_: &ClickEvent, window: &mut Window, cx: &mut App| act(name, &id, "", window, cx);

    // The title row: title, the mind map count, the paperclip, the menu (compact).
    let mut title = div().flex().items_start().gap(T::SP_8)
        .child(div().flex_1().min_w_0().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(cd.title.clone()));
    if cd.mm > 0 {
        title = title.child(w::tip(div().id(eid("kanban-mm", &sid)).flex().flex_none().items_center().gap(T::SP_2).h(T::SP_16 + T::SP_2).pl(T::SP_4).pr(T::SP_6).mt(T::SP_2)
            .rounded(T::R_PILL).bg(T::accent_dim()).text_color(T::accent_hover()).text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).cursor_pointer()
            .child(w::icon(IconName::Share2, T::ICON_SM)).child(cd.mm.to_string())
            .on_click(a("goto-mindmap", id.clone())), "Open in the Mindmap"));
    }
    let tid = id.clone();
    let clip_btn = if cd.media > 0 { w::toggle(eid("kanban-clip", &sid), cd.media.to_string(), clip_open).icon(IconName::Paperclip) }
        else { w::icon_toggle(eid("kanban-clip", &sid), IconName::Paperclip, "Attachments", clip_open) };
    title = title.child(clip_btn.on_click(move |e, _, cx| {
        let p = e.position();
        super::clip_open(&tid, f32::from(p.x + T::SP_12), f32::from(p.y + T::SP_12), f32::from(card_w), cx);
    }));
    if cd.compact { title = title.child(w::icon_toggle(eid("kanban-menu-c", &sid), IconName::Menu, "Task actions", cd.menu_open).on_click(a("card-menu", id.clone()))); }

    let mut col = div().flex().flex_col().px(T::SP_12).pt(T::SP_12).pb(T::SP_10).child(title);
    if !cd.desc.is_empty() { col = col.child(div().w_full().min_w_0().text_size(T::TEXT_12).text_color(T::text_secondary()).child(cd.desc.clone())); }

    // Subtasks, the new-subtask row, and the progress line.
    if cd.total > 0 || cd.adding {
        let mut list = div().flex().flex_col().pt(T::SP_8).gap(px(1.));
        for (n, td) in cd.todos.iter().enumerate() { list = list.child(todo_row(&cd.id, n, td)); }
        if cd.adding {
            if let Some(i) = k.todo_input.clone() {
                list = list.child(div().flex().items_center().gap(T::SP_8).pl(T::SP_6).pr(T::SP_4).py(T::SP_4).rounded(T::R_CONTROL)
                    .bg(T::bg_tertiary()).border_1().border_color(T::accent_edge())
                    .capture_key_down(|e, window, cx| if e.keystroke.key == "escape" { super::cancel_line(true, window, cx); })
                    .child(w::check(eid("kanban-ghost", &sid), "", false, |_, _, _| {}))
                    .child(div().flex_1().child(w::input(&i))));
            }
        }
        col = col.child(list);
    }
    if cd.total > 0 {
        let frac = cd.done as f32 / cd.total.max(1) as f32;
        col = col.child(div().flex().items_center().gap(T::SP_8).pt(T::SP_8)
            .child(div().flex_1().h(T::SP_2 + px(1.)).rounded(T::R_PILL).bg(T::bg_tertiary()).overflow_hidden()
                .child(div().h_full().w(relative(frac)).rounded(T::R_PILL).bg(if cd.done == cd.total { T::green() } else { T::accent() })))
            .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(format!("{} of {}", cd.done, cd.total))));
    }
    // The footer: the menu, then + Subtask (not on a compact card).
    if !cd.compact {
        col = col.child(div().flex().items_center().gap(T::SP_6).pt(T::SP_10)
            .child(w::icon_toggle(eid("kanban-menu", &sid), IconName::Menu, "Task actions", cd.menu_open).on_click(a("card-menu", id.clone())))
            .child(w::button(eid("kanban-sub", &sid), "Subtask").icon(IconName::Plus).tooltip("Add a subtask").on_click(a("add-subtask", id.clone()))));
    }
    // The card's actions, under its menu.
    if cd.menu_open {
        let edit = if fold {  w::icon_button(eid("kanban-edit", &sid), IconName::Pencil, "Edit task") } else {  w::button(eid("kanban-edit", &sid), "Edit").icon(IconName::Pencil).tooltip("Edit task") };
        let del = if fold {  w::icon_button(eid("kanban-del", &sid), IconName::Trash, "Delete task") } else {  w::ghost(eid("kanban-del", &sid), "Delete").icon(IconName::Trash).tooltip("Delete task") };
        col = col.child(div().flex().flex_col().gap(T::SP_8).pt(T::SP_8).child(w::rule())
            .child(div().flex().items_center().gap(T::SP_6)
                .when(cd.compact, |d| d.child(w::button(eid("kanban-sub2", &sid), "Subtask").icon(IconName::Plus).tooltip("Add a subtask").on_click(a("add-subtask", id.clone()))))
                .child(w::icon_button(eid("kanban-brain", &sid), IconName::Share2, "Brainstorm this task in the Mindmap").on_click(a("brainstorm", id.clone())))
                .child(edit.on_click(a("edit-task", id.clone())))
                .child(div().flex_1())
                .child(del.on_click(a("del-task", id.clone())))));
    }
    let title_text = cd.title.clone();
    div().id(eid("kanban-card", &sid)).relative().flex_none().rounded(T::R_CARD).border_1().bg(T::bg_card())
        .border_color(T::border()).hover(|d| d.border_color(T::kanban_slate_edge()))
        .when(dragged, |d| d.opacity(T::KANBAN_O_DIM))
        .cursor(if dragged { CursorStyle::ClosedHand } else { CursorStyle::OpenHand })
        .child(measure(move |b, k| k.card_boxes.push((st_, id2, b))))
        // A press anywhere arms a drag; past a few px it moves (kanban.rs pointer_move).
        .on_mouse_down(MouseButton::Left, move |e, _, cx| super::press(&id, &title_text, e.position, cx))
        .child(col)
        .into_any_element()
}

/// One subtask row (.kanban-todo): checkbox, text, media count, "…".
fn todo_row(task: &str, n: usize, td: &board::Todo) -> AnyElement {
    let key = SharedString::from(format!("{task}-{}", td.id));
    let (t1, d1, done) = (task.to_string(), td.id.clone(), td.done);
    let (t2, d2) = (task.to_string(), td.id.clone());
    let row = div().id(eid("kanban-todo", &key)).group("kanban-todo").flex().items_start().gap(T::SP_8).pl(T::SP_6).pr(T::SP_4).py(T::SP_4)
        .rounded(T::R_CONTROL).hover(|d| d.bg(T::bg_hover()))
        .child(div().pt(T::SP_2).child(w::check(eid("kanban-tick", &key), "", done, move |v, _, cx| super::toggle_todo(&t1, &d1, *v, cx))))
        .child(div().flex_1().min_w_0().text_size(T::TEXT_13).text_color(if done { T::text_muted() } else { T::text() }).child(td.text.clone()))
        .when(td.media > 0, |d| d.child(w::button(eid("kanban-tmedia", &key), td.media.to_string()).icon(IconName::Paperclip).on_click(|_, _, cx| super::media_modal(cx))))
        .child(div().opacity(T::KANBAN_O_CARET).group_hover("kanban-todo", |d| d.opacity(1.))
            .child(w::icon_button(eid("kanban-tmore", &key), IconName::Ellipsis, "More").on_click(move |e, _, cx| {
                let p = e.position();
                super::todo_menu(&t2, &d2, f32::from(p.x + T::SP_12), f32::from(p.y + T::SP_12), cx);
            })));
    let _ = n;
    row.into_any_element()
}

// ─── Trash and History ───

fn section(text: &'static str) -> Div {
    div().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(text)
}

fn hist_row(id: impl Into<ElementId>, ts: String, text: String, sub: String, restore: Option<Box<dyn Fn(&mut Window, &mut App)>>) -> AnyElement {
    div().id(id).flex().items_center().gap(T::SP_8).py(T::SP_4).border_b_1().border_color(T::border_subtle())
        .when(!ts.is_empty(), |d| d.child(div().flex_none().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child(ts)))
        .child(div().flex_1().min_w_0().text_size(T::TEXT_12).text_color(T::text_secondary()).child(text))
        .when(!sub.is_empty(), |d| d.child(div().flex_none().max_w(T::KANBAN_COL_MIN).truncate().text_size(T::TEXT_11).text_color(T::text_muted()).child(sub)))
        .when_some(restore, |d, f| d.child(w::button("restore", "Restore").on_click(move |_, window, cx| f(window, cx))))
        .into_any_element()
}

fn trash(cx: &App) -> AnyElement {
    let (dt, dd) = st(cx).board.as_ref().map(board::trash).unwrap_or_default();
    let mut l = div().id("kanban-trash").size_full().overflow_y_scroll().p(T::SP_12).flex().flex_col().gap(T::SP_8)
        .child(section("DELETED TASKS"));
    if dt.is_empty() { l = l.child(muted("No deleted tasks.")); }
    for (i, t) in dt.into_iter().enumerate() {
        let id = t.task.clone();
        l = l.child(div().flex().flex_col().gap(T::SP_8).px(T::SP_12).pt(T::SP_12).pb(T::SP_10).rounded(T::R_CARD).border_1().border_color(T::border()).bg(T::bg_card())
            .child(div().w_full().min_w_0().opacity(T::KANBAN_O_SOFT).text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(t.text))
            .child(div().flex().child(w::button(("kanban-restore", i), "Restore").icon(IconName::RotateCcw).on_click(move |_, window, cx| act("restore-task", &id, "", window, cx)))));
    }
    l = l.child(div().h(T::SP_8)).child(section("DELETED TODOS"));
    if dd.is_empty() { l = l.child(muted("No deleted todos.")); }
    for (i, t) in dd.into_iter().enumerate() {
        let (task, todo) = (t.task.clone(), t.todo.clone());
        l = l.child(div().id(("kanban-dtodo", i)).child(hist_row(("kanban-dtodo-row", i), String::new(), t.text, format!("in {}", t.sub),
            Some(Box::new(move |window, cx| act("restore-todo", &task, &todo, window, cx))))));
    }
    l.into_any_element()
}

fn history(cx: &App) -> AnyElement {
    let h = st(cx).board.as_ref().map(|b| board::history(b, board::local_time)).unwrap_or_default();
    let mut l = div().id("kanban-history").size_full().overflow_y_scroll().p(T::SP_12).flex().flex_col();
    if h.is_empty() { l = l.child(muted("No history yet.")); }
    for (i, r) in h.into_iter().enumerate() { l = l.child(hist_row(("kanban-hist", i), r.sub, r.text, String::new(), None)); }
    l.into_any_element()
}

// ─── The layers over the window ───

/// The dragged card's clone (.drag-clone): its box and title, following the pointer.
fn drag_clone(cx: &App) -> Option<AnyElement> {
    let d = st(cx).drag.as_ref()?;
    Some(deferred(anchored().position(point(px(d.x - d.grab.0), px(d.y - d.grab.1))).child(
        div().w(px(d.w)).h(px(d.h)).overflow_hidden().opacity(T::KANBAN_O_CLONE).p(T::SP_12).rounded(T::R_CARD).border_1().border_color(T::accent()).bg(T::bg_card())
            .shadow_lg().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(d.title.clone()),
    )).with_priority(1).into_any_element())
}

/// A card's attachments (.kc-media-pop): thumbnails (or "No attachments yet") and the + that opens
/// the task's media, under the paperclip. A press outside closes it.
fn clip_pop(window: &Window, cx: &App) -> Option<AnyElement> {
    let k = st(cx);
    let Clip { task, x, y, w: cw } = k.clip.as_ref()?;
    let vp = window.viewport_size();
    let pw = (px(*cw) - T::SP_8).clamp(T::KANBAN_POP_MIN, T::KANBAN_POP_MAX);
    let ph = T::KANBAN_THUMB + T::SP_8 * 2.;
    let px_ = (px(*x) - pw).min(vp.width - pw - T::SP_8).max(T::SP_8);
    let py_ = if px(*y) + ph > vp.height - T::SP_8 { (px(*y) - ph - T::SP_32).max(T::SP_8) } else { px(*y) };
    let items = k.media_task.get(task).cloned().unwrap_or_default();
    let mut strip = div().id("kanban-clip-strip").flex_1().min_w_0().overflow_x_scroll().flex().items_center().gap(T::SP_6).p(T::SP_8);
    if items.is_empty() { strip = strip.child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("No attachments yet")); }
    for (i, it) in items.iter().enumerate() {
        let id = it.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let kind = it.get("kind").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let pic = k.thumbs.get(&id).cloned().flatten();
        let icon = match kind.as_str() { "video" => IconName::Film, "audio" => IconName::Music, "pdf" => IconName::FileText, "image" => IconName::Image, _ => IconName::File };
        strip = strip.child(div().id(("kanban-thumb", i)).flex_none().size(T::KANBAN_THUMB).rounded(T::R_CONTROL).border_1().border_color(T::border())
            .hover(|d| d.border_color(T::accent())).bg(T::bg_tertiary()).overflow_hidden().cursor_pointer().flex().items_center().justify_center()
            .text_color(T::text_secondary())
            .map(|d| match pic { Some(p) => d.child(img(ImageSource::Image(p)).size_full().object_fit(ObjectFit::Cover)), None => d.child(w::icon(icon, T::ICON_XL)) })
            .on_click(move |_, _, cx| super::open_media(&id, cx)));
    }
    let vw = vp.width; let vh = vp.height;
    Some(deferred(anchored().position(point(px(0.), px(0.))).child(
        div().id("kanban-clip-back").occlude().w(vw).h(vh)
            .on_mouse_down(MouseButton::Left, |_, _, cx| super::clip_close(cx))
            .child(div().id("kanban-clip-pop").absolute().left(px_).top(py_).w(pw).h(ph).flex().rounded(T::R_FIELD).border_1().border_color(T::border())
                .bg(T::bg_secondary()).shadow_lg().overflow_hidden()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(strip)
                .child(div().flex_none().h_full().border_l_1().border_color(T::border_subtle()).flex().items_center().px(T::SP_4)
                    .child(w::square_button("kanban-clip-add", IconName::Plus, "Attach media", w::SquareLook::Plain, T::SP_40, T::ICON_XL)
                        .on_click(|_, _, cx| super::media_modal(cx))))),
    )).with_priority(1).into_any_element())
}
