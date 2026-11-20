//! Notes drawn (native-gpui's notes_view.rs, the same parts, arrangement and
//! states), every control from the widgets crate, the theme's tokens only.
//! The state and the rules are mod.rs's; this file reads them through the
//! store's handle (place_row, tab_list) and calls them.
//!
//! The tab (mountManager): its bar (Notes; A-, reset, A+), the page list
//! (+ New page, Search pages, Pages | Trash) and the page open beside it
//! (over it when narrow). A workbench pane ("panel:notes:<n>"): the picker
//! (mountPicker) until a page is picked, then that page alone; its Pages and
//! text size buttons in a row of their own (the pane header has no notes
//! tools).
//!
//! On v2's rules: the tab's view is a layout over three cached leaves (the
//! bar, the list, the page); a place's view is one cached leaf. Its page,
//! its picker and the tab's list are `gpui::list`s: only the rows on screen
//! are built, and a change splices only the rows whose content changed (a
//! signature per row). A block's formatted text is parsed once per change
//! of its source (`rich`, kept by block), never per frame.
//!
//! GPUI has no rich-text editor: a text block is edited as its source
//! (doc.rs) in a text box while it has the caret, and drawn formatted
//! otherwise. One text box per place serves whichever block (or table cell)
//! has the caret.

use super::{doc, BlockRow, Item, PlaceRow, S};
use crundi_theme::{IconName, T};
use crundi_widgets::{self as w, HeadContent as C, HeadLook as L, InputEvent, InputState, TextareaState};
use gpui::{prelude::FluentBuilder as _, *};
use std::{collections::HashMap, hash::{Hash, Hasher}, rc::Rc};

fn sid(s: impl Into<String>) -> ElementId { ElementId::Name(SharedString::from(s.into())) }

fn sig(x: &impl Hash) -> u64 { let mut h = std::collections::hash_map::DefaultHasher::new(); x.hash(&mut h); h.finish() }

/// Splice a list for what changed between two signature vectors (the
/// common head and tail kept, measured as they were).
fn splice_diff(list: &ListState, old: &[u64], new: &[u64]) {
    let pre = old.iter().zip(new.iter()).take_while(|(a, b)| a == b).count();
    if pre == old.len() && pre == new.len() { return; }
    let max_suf = old.len().min(new.len()) - pre;
    let suf = old.iter().rev().zip(new.iter().rev()).take(max_suf).take_while(|(a, b)| a == b).count();
    list.splice(pre..old.len() - suf, new.len() - pre - suf);
}

// ─── The views ───

/// The view of a place (the tab's page: key ""; a pane: its key), made once.
pub(crate) fn place_view(key: &str, window: &mut Window, cx: &mut App) -> Entity<PlaceView> {
    let st = super::store(cx);
    if let Some(v) = st.read(cx).views.get(key).cloned() { return v; }
    let k = key.to_string();
    let v = cx.new(|cx| PlaceView::new(k, st.clone(), window, cx));
    st.update(cx, |s, _| { s.views.insert(key.to_string(), v.clone()); });
    let k = key.to_string();
    cx.defer(move |cx| super::attach(&k, cx));
    v
}

/// The tab's view (a layout over its cached bar, list and page).
pub(crate) fn tab_view(window: &mut Window, cx: &mut App) -> Entity<TabView> {
    let st = super::store(cx);
    let place = place_view("", window, cx);
    let side = match st.read(cx).side.clone() { Some(s) => s, None => { let s = cx.new(|cx| SideView::new(st.clone(), window, cx)); st.update(cx, |x, _| x.side = Some(s.clone())); s } };
    let bar = match st.read(cx).bar.clone() { Some(b) => b, None => { let b = cx.new(|_| TabBar { store: st.clone() }); st.update(cx, |x, _| x.bar = Some(b.clone())); b } };
    cx.new(|_| TabView { store: st, place, side, bar, width: 0. })
}

/// The Notes tab: a layout (no text of its own): the bar over the list and
/// the page, side by side (stacked when narrow).
pub(crate) struct TabView {
    store: Entity<S>,
    place: Entity<PlaceView>,
    side: Entity<SideView>,
    bar: Entity<TabBar>,
    width: f32,
}

impl Render for TabView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("notes-tab");
        let project = self.store.read(cx).tab_project;
        let stack = self.width > 0. && self.width < f32::from(T::NOTES_STACK);
        let me = cx.entity().downgrade();
        let full = || StyleRefinement::default().size_full();
        let body = if !project {
            // (No project: the page's place says so; the list is not shown.)
            div().flex_1().min_h_0().child(self.place.clone().cached(full())).into_any_element()
        } else if stack {
            div().flex_1().min_h_0().flex().flex_col()
                .child(div().flex_none().h(relative(T::NOTES_SIDE_SHARE)).child(self.side.clone().cached(full())))
                .child(div().flex_none().h(px(1.)).bg(T::border()))
                .child(div().flex_1().min_h_0().child(self.place.clone().cached(full())))
                .into_any_element()
        } else {
            div().flex_1().min_h_0().flex()
                .child(div().flex_none().w(T::NOTES_SIDE_W).h_full().child(self.side.clone().cached(full())))
                .child(div().flex_none().w(px(1.)).h_full().bg(T::border()))
                .child(div().flex_1().min_w_0().h_full().child(self.place.clone().cached(full())))
                .into_any_element()
        };
        div().relative().size_full().flex().flex_col().bg(T::bg())
            .child(canvas(move |b, _, cx| {
                let wv = f32::from(b.size.width);
                if let Some(me) = me.upgrade() { me.update(cx, |t, cx| if (t.width - wv).abs() > 1. { t.width = wv; cx.notify(); }); }
            }, |_, _, _, _| {}).absolute().size_full())
            .child(self.bar.clone().cached(StyleRefinement::default().w_full().flex_none().h(T::SP_24 + T::SP_6 * 2. + px(1.))))
            .child(body)
    }
}

/// The tab's bar: Notes; A-, reset, A+ (a cached leaf).
pub(crate) struct TabBar { #[allow(dead_code)] store: Entity<S> }

impl Render for TabBar {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("notes-bar");
        div().size_full().flex().items_center().gap(T::SP_6).px(T::SP_12).py(T::SP_6).border_b_1().border_color(T::border())
            .child(div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child("Notes"))
            .child(div().flex_1())
            .child(font_buttons("tab"))
    }
}

/// The tab's page list (.nt-side): + New page, the search, Pages | Trash,
/// the pages (a virtualised list).
pub(crate) struct SideView {
    store: Entity<S>,
    search: Entity<InputState>,
    list: ListState,
    items: Rc<Vec<Item>>,
    trash: bool,
    _subs: Vec<Subscription>,
}

impl SideView {
    fn new(store: Entity<S>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search pages"));
        let subs = vec![cx.subscribe(&search, |_, st, ev: &InputEvent, cx| if let InputEvent::Change = ev { let q = st.read(cx).value().to_string(); super::search("", &q, cx); })];
        SideView { store, search, list: ListState::new(0, ListAlignment::Top, T::SP_48 * 4.), items: Rc::new(vec![]), trash: false, _subs: subs }
    }
}

impl Render for SideView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("notes-side");
        let (items, msg, view, new_busy) = { let s = self.store.read(cx); let (i, m) = super::tab_list(s); let (_, v, _, b) = super::tab_state(s); (i, m, v, b) };
        let trash = view == "trash";
        if trash != self.trash { self.trash = trash; self.list.reset(items.len()); self.items = Rc::new(items); }
        else if *self.items != items {
            let old: Vec<u64> = self.items.iter().map(item_sig).collect();
            let new: Vec<u64> = items.iter().map(item_sig).collect();
            splice_diff(&self.list, &old, &new);
            self.items = Rc::new(items);
        }
        let rows = self.items.clone();
        let n = rows.len();
        let body = if !msg.is_empty() && n == 0 {
            div().flex_1().min_h_0().px(T::SP_10).child(list_msg(msg)).into_any_element()
        } else {
            list(self.list.clone(), move |i, _, _| {
                let Some(it) = rows.get(i) else { return div().into_any_element() };
                div().w_full().px(T::SP_10).pt(if i == 0 { px(0.) } else { T::SP_4 }).when(i + 1 == n, |d| d.pb(T::SP_12))
                    .child(side_item(it, trash)).into_any_element()
            }).flex_1().min_h_0().into_any_element()
        };
        div().size_full().flex().flex_col().bg(T::bg_secondary())
            .child(div().flex().flex_col().flex_none().gap(T::SP_8).p(T::SP_12)
                .child(new_page_button("nt-new-tab", new_busy, |cx| super::new_page("", cx)))
                .child(w::input(&self.search))
                .child(w::segmented("nt-view", &["Pages", "Trash"], if trash { 1 } else { 0 }, |i, _, cx| super::set_view(if *i == 1 { "trash" } else { "pages" }, cx))))
            .child(body)
    }
}

fn item_sig(i: &Item) -> u64 { sig(&(&i.id, &i.title, &i.preview, &i.meta, i.busy, i.cur)) }

/// One page in the tab's list: a click opens it, a right click its menu;
/// in the Trash, Restore and Delete for good.
fn side_item(it: &Item, trash: bool) -> AnyElement {
    let id = it.id.clone();
    let mut row = w::page_item(sid(format!("nt-ti-{}", it.id)), it.title.clone(), it.preview.clone(), it.meta.clone(), it.cur, it.busy, true, !trash);
    if !trash {
        let (a, b) = (id.clone(), id.clone());
        row = row.on_click(move |_, _, cx| super::pick("", &a, cx))
            .on_mouse_down(MouseButton::Right, move |e, _, cx| super::item_menu(&b, f32::from(e.position.x), f32::from(e.position.y), cx));
        if it.busy { row = w::tip(row, "Already open in another pane: one page, one pane"); }
    } else {
        let (a, b) = (id.clone(), id.clone());
        row = row.child(div().flex().gap(T::SP_6).pt(T::SP_4)
            .child(w::button(sid(format!("nt-restore-{id}")), "Restore").on_click(move |_, _, cx| super::trash_act("restore", &a, cx)))
            .child(w::button(sid(format!("nt-purge-{id}")), "Delete for good").on_click(move |_, _, cx| super::trash_act("purge", &b, cx))));
    }
    row.into_any_element()
}

/// A block's formatted text, parsed once per source: the text, its styles,
/// its links.
struct Rich { text: SharedString, hl: Vec<(std::ops::Range<usize>, HighlightStyle)>, links: Vec<(std::ops::Range<usize>, String)> }

fn parse_rich(src: &str, done: bool) -> Rich {
    let runs = doc::src_to_runs(src);
    let mut text = String::new();
    let mut hl: Vec<(std::ops::Range<usize>, HighlightStyle)> = vec![];
    let mut links: Vec<(std::ops::Range<usize>, String)> = vec![];
    for r in &runs {
        let a = text.len();
        text.push_str(&r.text);
        let b = text.len();
        if a == b { continue; }
        let st = &r.st;
        let mut h = HighlightStyle::default();
        if st.b { h.font_weight = Some(T::W_BOLD); }
        if st.i { h.font_style = Some(FontStyle::Italic); }
        if st.u || st.href.is_some() { h.underline = Some(UnderlineStyle { thickness: px(1.), color: None, wavy: false }); }
        if st.s || done { h.strikethrough = Some(StrikethroughStyle { thickness: px(1.), color: None }); }
        if st.code { h.background_color = Some(T::bg_tertiary()); h.color = Some(T::error_text()); }
        if let Some(href) = &st.href { h.color = Some(T::accent_hover()); links.push((a..b, href.clone())); }
        if h != HighlightStyle::default() { hl.push((a..b, h)); }
    }
    if done && runs.is_empty() { hl.clear(); }
    if text.is_empty() { text.push('\u{a0}'); }
    Rich { text: text.into(), hl, links }
}

/// One place: the picker, the page, or a line; in a pane, its tools' row.
pub(crate) struct PlaceView {
    key: String,
    store: Entity<S>,
    title_in: Entity<TextareaState>,
    edit_in: Entity<TextareaState>,
    search_in: Entity<InputState>,
    query_in: Entity<InputState>,
    link_in: Entity<InputState>,
    sel_focus: FocusHandle,
    div_focus: FocusHandle,
    applied_focus: i32,
    applied_title: i32,
    menu_gen: u64,
    fmt_gen: u64,
    /// The block (row, column for a cell; -2 a divider) the text box serves.
    edit: Option<(String, i32, i32)>,
    editing: bool,
    last_sel: (usize, usize),
    width: f32,
    /// The page (title, blocks, the tail) and the picker (its head, the
    /// pages): virtualised, with each row's signature as last drawn.
    page: ListState,
    page_sigs: Vec<u64>,
    pick: ListState,
    pick_sigs: Vec<u64>,
    mode: String,
    /// The page on show (a new one lets the old one's caret go).
    shown_page: String,
    /// Formatted text by block / cell ("id-r-c"): its source, done, the parse.
    rich: HashMap<String, (String, bool, Rc<Rich>)>,
    _subs: Vec<Subscription>,
}

impl PlaceView {
    fn new(key: String, store: Entity<S>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title_in = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 6));
        let edit_in = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 400));
        let search_in = cx.new(|cx| InputState::new(window, cx).placeholder("Search pages"));
        let query_in = cx.new(|cx| InputState::new(window, cx).placeholder("Search languages"));
        let link_in = cx.new(|cx| InputState::new(window, cx).placeholder("Paste or type a link, Enter to apply"));
        let subs = vec![
            cx.subscribe(&title_in, |this, st, ev: &InputEvent, cx| {
                let k = this.key.clone();
                match ev {
                    InputEvent::Change => {
                        let t = st.read(cx).value().to_string();
                        let cur = super::place_row(this.store.read(cx), &k).map(|p| p.title).unwrap_or_default();
                        if t != cur { super::title_edited(&k, &t, cx); }
                    }
                    InputEvent::Focus => super::focused(&k, "title", -1, -1, cx),
                    _ => {}
                }
            }),
            cx.subscribe(&edit_in, |this, st, ev: &InputEvent, cx| {
                let k = this.key.clone();
                let Some((id, r, c)) = this.edit.clone() else { return };
                match ev {
                    InputEvent::Change => {
                        let (t, caret) = { let s = st.read(cx); (s.value().to_string(), s.cursor()) };
                        if super::src_of(&k, &id, r, c, cx).as_deref() == Some(t.as_str()) { return; }
                        if r >= 0 { super::cell_edited(&k, &id, r, c, &t, cx); } else { super::edited(&k, &id, &t, caret as i32, cx); }
                    }
                    InputEvent::Focus => { this.editing = true; super::focused(&k, &id, r, c, cx); }
                    InputEvent::Blur => { this.editing = false; super::blur(&k, &id, cx); cx.notify(); }
                    _ => {}
                }
            }),
            cx.observe(&edit_in, |this, st, cx| {
                // The selection moved: the format bar follows it (sel_moved).
                let Some((id, _, _)) = this.edit.clone() else { return };
                if !this.editing { return; }
                let (range, rev, caret) = { let s = st.read(cx); (s.selected_range(), s.is_selection_reversed(), s.cursor_layout()) };
                let sel = (range.start, range.end);
                if sel == this.last_sel { return; }
                this.last_sel = sel;
                let (a, c) = if rev { (range.end, range.start) } else { (range.start, range.end) };
                let (x, y) = caret.map(|(b, _)| (f32::from(b.origin.x), f32::from(b.origin.y))).unwrap_or_else(|| this.store.read(cx).caret_xy);
                let k = this.key.clone();
                cx.defer(move |cx| super::sel_moved(&k, &id, a as i32, c as i32, x, y, cx));
            }),
            cx.subscribe(&search_in, |this, st, ev: &InputEvent, cx| if let InputEvent::Change = ev { let q = st.read(cx).value().to_string(); let k = this.key.clone(); super::search(&k, &q, cx); }),
            cx.subscribe(&query_in, |this, st, ev: &InputEvent, cx| if let InputEvent::Change = ev {
                let q = st.read(cx).value().to_string();
                if this.store.read(cx).menu.query != q { super::menu_query(&q, cx); }
            }),
            cx.subscribe(&link_in, |_, st, ev: &InputEvent, cx| if let InputEvent::PressEnter { .. } = ev { let u = st.read(cx).value().to_string(); super::link_apply(&u, cx); }),
        ];
        PlaceView {
            key, store, title_in, edit_in, search_in, query_in, link_in, sel_focus: cx.focus_handle(), div_focus: cx.focus_handle(),
            applied_focus: 0, applied_title: -1, menu_gen: 0, fmt_gen: 0, edit: None, editing: false, last_sel: (0, 0), width: 0.,
            page: ListState::new(0, ListAlignment::Top, T::SP_48 * 8.), page_sigs: vec![], pick: ListState::new(0, ListAlignment::Top, T::SP_48 * 4.), pick_sigs: vec![],
            mode: String::new(), shown_page: String::new(), rich: HashMap::new(), _subs: subs,
        }
    }

    /// The model's requests: the caret into a block, a cell, the title or the
    /// selection; the title shown again.
    fn apply(&mut self, p: &PlaceRow, window: &mut Window, cx: &mut Context<Self>) {
        // The block the text box served is gone (another page, the picker):
        // its caret goes with it, as the web's element leaves the page.
        let gone = p.mode != "editor" || self.edit.as_ref().is_some_and(|e| !p.blocks.iter().any(|b| b.b.id == e.0));
        if gone && self.edit.is_some() {
            self.edit = None;
            self.editing = false;
            if self.edit_in.focus_handle(cx).is_focused(window) || self.div_focus.is_focused(window) { window.blur(cx); }
        }
        if p.page_id != self.shown_page {
            self.shown_page = p.page_id.clone();
            if self.title_in.focus_handle(cx).is_focused(window) || self.sel_focus.is_focused(window) { window.blur(cx); }
        }
        if p.title_gen != self.applied_title {
            self.applied_title = p.title_gen;
            let t = p.title.clone();
            if self.title_in.read(cx).value().as_ref() != t.as_str() { self.title_in.update(cx, |st, cx| st.set_value(t, window, cx)); }
        }
        if p.focus_gen != self.applied_focus {
            self.applied_focus = p.focus_gen;
            let (id, r, c, a, b) = p.focus.clone();
            let (a, b) = (a.max(0) as usize, b.max(0) as usize);
            if id == "title" {
                let n = self.title_in.read(cx).value().len();
                self.title_in.update(cx, |st, cx| { st.set_selected_range(a.min(n)..b.min(n), cx); st.focus(window, cx); });
                self.page.scroll_to_reveal_item(0);
            } else if id == "__sel" {
                self.edit = None;
                self.editing = false;
                window.focus(&self.sel_focus, cx);
            } else if let Some((ix, row)) = p.blocks.iter().enumerate().find(|(_, x)| x.b.id == id) {
                self.page.scroll_to_reveal_item(ix + 1);
                if row.b.kind == "divider" {
                    self.edit = Some((id.clone(), -2, -2));
                    self.editing = false;
                    window.focus(&self.div_focus, cx);
                    let k = self.key.clone();
                    cx.defer(move |cx| super::focused(&k, &id, -1, -1, cx));
                } else {
                    let src = if r >= 0 { row.b.rows.get(r as usize).and_then(|x| x.get(c as usize)).cloned().unwrap_or_default() } else { row.b.text.clone() };
                    let n = src.len();
                    let was = self.edit.replace((id.clone(), r, c));
                    let same = was.as_ref() == Some(&(id.clone(), r, c)) && self.editing;
                    self.last_sel = (a.min(n), b.min(n));
                    self.edit_in.update(cx, |st, cx| {
                        if st.value().as_ref() != src.as_str() { st.set_value(src, window, cx); }
                        st.set_selected_range(a.min(n)..b.min(n), cx);
                        st.focus(window, cx);
                    });
                    if same {
                        // Still the same block: tell the model as a fresh focus would.
                        let k = self.key.clone();
                        cx.defer(move |cx| super::focused(&k, &id, r, c, cx));
                    }
                    self.editing = true;
                }
            }
        }
        // The menu's search, and the link box, take the caret as they open.
        let (mg, query_on, q, fg, link, lv, mine) = {
            let s = self.store.read(cx);
            (s.menu.gen_n, s.menu.query_on && s.menu.open, s.menu.query.clone(), s.fmt.gen_n, s.fmt.link && s.fmt.open, s.fmt.link_value.clone(), s.float.as_ref().is_some_and(|f| f.key() == self.key))
        };
        if mine && query_on && mg != self.menu_gen {
            self.menu_gen = mg;
            self.query_in.update(cx, |st, cx| { st.set_value(q, window, cx); st.focus(window, cx); });
        }
        if mine && link && fg != self.fmt_gen {
            self.fmt_gen = fg;
            self.link_in.update(cx, |st, cx| { st.set_value(lv, window, cx); st.select_all(window, cx); st.focus(window, cx); });
        } else if mine && !link { self.fmt_gen = fg; }
    }

    /// A key while one of this place's boxes has the caret: Some(handled).
    fn on_key(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) -> Option<bool> {
        let k = self.key.clone();
        if self.query_in.focus_handle(cx).is_focused(window) {
            return Some(matches!(name, "up" | "down" | "enter" | "tab" | "escape") && super::menu_key(name, cx));
        }
        if self.link_in.focus_handle(cx).is_focused(window) {
            if name == "escape" { super::link_cancel(cx); return Some(true); }
            return Some(false);
        }
        if self.title_in.focus_handle(cx).is_focused(window) { return Some(super::title_key(&k, name, cx)); }
        if self.sel_focus.is_focused(window) { return Some(super::sel_key(&k, name, cx)); }
        if self.div_focus.is_focused(window) {
            let Some((id, _, _)) = self.edit.clone() else { return Some(false) };
            return Some(super::divider_key(&k, &id, name, cx));
        }
        if self.edit_in.focus_handle(cx).is_focused(window) {
            let Some((id, r, c)) = self.edit.clone() else { return Some(false) };
            let (range, rev, text, caret_layout, ib) = { let s = self.edit_in.read(cx); (s.selected_range(), s.is_selection_reversed(), s.value().to_string(), s.cursor_layout(), s.input_bounds()) };
            let (a, cc) = if rev { (range.end, range.start) } else { (range.start, range.end) };
            if r >= 0 { return Some(super::cell_key(&k, &id, r, c, name, a as i32, cc as i32, cx)); }
            // On the first or last line (visually, from the caret's box; by
            // line breaks before the first layout).
            let (first, last) = match caret_layout {
                Some((b, lh)) if ib.size.height > px(0.) => (b.top() < ib.top() + lh, b.bottom() > ib.bottom() - lh),
                _ => (!text[..cc.min(text.len())].contains('\n'), !text[cc.min(text.len())..].contains('\n')),
            };
            return Some(super::text_key(&k, &id, name, a as i32, cc as i32, first, last, cx));
        }
        None
    }
}

/// The editor's keys, before the text box's own: true when one of the
/// screen's handlers took it.
pub(crate) fn key(name: &str, window: &mut Window, cx: &mut App) -> bool {
    let views: Vec<Entity<PlaceView>> = super::store(cx).read(cx).views.values().cloned().collect();
    for v in views {
        if let Some(h) = v.update(cx, |v, cx| v.on_key(name, window, cx)) { return h; }
    }
    false
}

/// A keystroke by native-gpui's names ("enter", "shift-up", "ctrl-b",
/// "ctrl-shift-a"…); "" for one the screen never takes.
pub(crate) fn key_name(k: &Keystroke) -> String {
    let m = &k.modifiers;
    if m.alt { return String::new(); }
    let ctrl = m.control || m.platform;
    let base = match k.key.as_str() {
        "enter" | "backspace" | "delete" | "up" | "down" | "tab" | "escape" => k.key.clone(),
        x if ctrl && x.chars().count() == 1 => x.to_lowercase(),
        _ => return String::new(),
    };
    format!("{}{}{}", if ctrl { "ctrl-" } else { "" }, if m.shift { "shift-" } else { "" }, base)
}

impl Render for PlaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("notes");
        let (p, hosted, dragging) = {
            let s = self.store.read(cx);
            (super::place_row(s, &self.key), s.head_hosted, s.drag.as_ref().is_some_and(|d| d.key == self.key))
        };
        let Some(p) = p else { return div().into_any_element() };
        self.apply(&p, window, cx);
        // A drag let go outside the page.
        if dragging && !cx.has_active_drag() { cx.defer(|cx| super::drag_end(false, cx)); }
        let key = self.key.clone();
        let me = cx.entity().downgrade();
        if p.mode != self.mode {
            // Another screen in this place: its lists start again.
            self.mode = p.mode.clone();
            self.page.reset(0); self.page_sigs.clear();
            self.pick.reset(0); self.pick_sigs.clear();
        }
        let p = Rc::new(p);
        let body = match p.mode.as_str() {
            "picker" => self.picker(p.clone(), cx).into_any_element(),
            "editor" => self.editor(p.clone(), window, cx).into_any_element(),
            _ => middle(p.msg.clone()).into_any_element(),
        };
        let mut root = div().relative().size_full().flex().flex_col().bg(T::bg())
            .child(canvas(move |b, _, cx| {
                let wv = f32::from(b.size.width);
                if let Some(me) = me.upgrade() { me.update(cx, |t, cx| if (t.width - wv).abs() > 1. { t.width = wv; cx.notify(); }); }
            }, |_, _, _, _| {}).absolute().size_full());
        // A pane: its tools in a row of their own (the pane header has none for notes).
        if !key.is_empty() && !hosted {
            let k = key.clone();
            root = root.child(div().flex().flex_none().items_center().gap(T::SP_2).px(T::SP_8).py(T::SP_2).border_b_1().border_color(T::border_subtle())
                .when(p.mode != "picker", |d| d.child(w::head_button(sid(format!("nt-pages-{key}")), C::Icon(IconName::Files), L::Rest, T::SP_24, T::SP_24, "Pages: open another page here")
                    .on_click(move |_, _, cx| super::switch_to_picker(&k, cx))))
                .child(div().flex_1())
                .child(font_buttons(&key)));
        }
        root.child(div().flex_1().min_h_0().child(body))
            .children(self.floats(window, cx))
            .into_any_element()
    }
}

// ─── Shared parts ───

/// A muted line in the middle (.nt-main-empty).
fn middle(text: impl Into<SharedString>) -> Div {
    div().size_full().flex().items_center().justify_center().px(T::SP_20).text_size(T::TEXT_13).text_color(T::text_muted()).child(div().text_center().child(text.into()))
}

fn list_msg(text: impl Into<SharedString>) -> Div {
    div().w_full().text_center().text_size(T::TEXT_12).text_color(T::text_muted()).child(text.into())
}

/// The text size buttons the tab bar and a pane share: A-, reset, A+ (all notes).
fn font_buttons(key: &str) -> Div {
    div().flex().flex_none().items_center().gap(T::SP_2)
        .child(w::head_button(sid(format!("nt-a-{key}")), C::Text("A-".into()), L::Rest, T::SP_24, T::SP_24, "Smaller text (all notes)").on_click(|_, _, cx| super::set_font(-1, cx)))
        .child(w::head_button(sid(format!("nt-a0-{key}")), C::Icon(IconName::RotateCcw), L::Rest, T::SP_24, T::SP_24, "Reset text size").on_click(|_, _, cx| super::set_font(0, cx)))
        .child(w::head_button(sid(format!("nt-a+-{key}")), C::Text("A+".into()), L::Rest, T::SP_24, T::SP_24, "Larger text (all notes)").on_click(|_, _, cx| super::set_font(1, cx)))
}

/// "+ New page" (.nt-new): dashed, accent on hover; faded while it works.
fn new_page_button(id: &str, busy: bool, f: impl Fn(&mut App) + 'static) -> Stateful<Div> {
    let b = w::dashed_button(sid(id.to_string()), "+ New page").font_weight(T::W_SEMIBOLD).text_color(T::text());
    if busy { b.opacity(T::NOTES_O_SOFT) } else { b.on_click(move |_, _, cx| f(cx)) }
}

// ─── The picker (.nt-pick) ───

impl PlaceView {
    /// The picker: its head (Notes, the line, + New page, the search) and
    /// the pages, one virtualised list that scrolls as one column.
    fn picker(&mut self, p: Rc<PlaceRow>, cx: &mut Context<Self>) -> AnyElement {
        let mut sigs = vec![sig(&(p.new_busy, &p.list_msg))];
        sigs.extend(p.items.iter().map(item_sig));
        splice_diff(&self.pick, &self.pick_sigs, &sigs);
        self.pick_sigs = sigs;
        let n = p.items.len();
        div().id(sid(format!("nt-pick-{}", p.key))).size_full().bg(T::bg())
            .child(list(self.pick.clone(), cx.processor(move |this: &mut PlaceView, i: usize, _, _| {
                let col = div().w_full().max_w(T::NOTES_PICK_MAX + T::SP_20 * 2.).mx_auto().px(T::SP_20);
                if i == 0 {
                    let k2 = p.key.clone();
                    let mut head = col.pt(T::SP_20).flex().flex_col()
                        .child(div().text_size(T::TEXT_16).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Notes"))
                        .child(div().h(T::SP_4))
                        .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Start a new page, or open one of this project\u{2019}s."))
                        .child(div().h(T::SP_12 + T::SP_2))
                        .child(new_page_button(&format!("nt-new-{}", p.key), p.new_busy, move |cx| super::new_page(&k2, cx)))
                        .child(div().h(T::SP_12))
                        .child(w::input(&this.search_in))
                        .child(div().h(T::SP_8));
                    if !p.list_msg.is_empty() { head = head.child(list_msg(p.list_msg.clone())); }
                    return head.into_any_element();
                }
                let Some(it) = p.items.get(i - 1) else { return div().into_any_element() };
                col.when(i > 1, |d| d.pt(T::SP_4)).when(i == n, |d| d.pb(T::SP_20)).child(pick_item(&p.key, it)).into_any_element()
            })).size_full())
            .into_any_element()
    }
}

fn pick_item(key: &str, it: &Item) -> Stateful<Div> {
    let row = w::page_item(sid(format!("nt-pi-{key}-{}", it.id)), it.title.clone(), it.preview.clone(), it.meta.clone(), false, it.busy, false, true);
    if it.busy { return w::tip(row, "Already open in another pane: one page, one pane"); }
    let (k, id) = (key.to_string(), it.id.clone());
    row.on_click(move |_, _, cx| super::pick(&k, &id, cx))
}

// ─── The editor (mountEditor, .nt-root) ───

/// What a page row draws from besides its block: the place's text size and
/// width, whether its text box or the divider's focus is there, the table's
/// buttons.
#[derive(Hash)]
struct RowLook { scale: u32, narrow: bool, here: Option<(i32, i32)>, cur: bool, div_on: bool }

impl PlaceView {
    fn editor(&mut self, p: Rc<PlaceRow>, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let k = super::scale_of(self.store.read(cx));
        let narrow = self.width > 0. && self.width < f32::from(T::NOTES_STACK);
        let margin = if narrow { T::SP_32 } else { T::SP_40 };
        let key = p.key.clone();
        // Each row's signature: what it draws; a changed one is measured again.
        let title_v = self.title_in.read(cx).value().to_string();
        let mut sigs = Vec::with_capacity(p.blocks.len() + 2);
        sigs.push(sig(&(&title_v, k.to_bits(), narrow)));
        for r in &p.blocks {
            let here = self.edit.as_ref().filter(|e| e.0 == r.b.id && self.editing).map(|e| (e.1, e.2));
            let div_on = self.edit.as_ref().is_some_and(|e| e.0 == r.b.id && e.1 == -2) && self.div_focus.is_focused(window);
            sigs.push(sig(&(r, RowLook { scale: k.to_bits(), narrow, here, cur: p.cur_id == r.b.id, div_on })));
        }
        sigs.push(sig(&narrow));
        splice_diff(&self.page, &self.page_sigs, &sigs);
        self.page_sigs = sigs;
        let nb = p.blocks.len();
        let p2 = p.clone();
        let page_list = list(self.page.clone(), cx.processor(move |this: &mut PlaceView, i: usize, window, cx| {
            let p = &p2;
            let col = div().w_full().max_w(T::NOTES_PAGE_MAX + margin).mx_auto()
                .pr(if narrow { T::SP_16 } else { T::SP_24 }).pl(if narrow { T::SP_2 } else { T::SP_16 });
            if i == 0 {
                let title_fs = (T::TEXT_26 + T::SP_2) * k;
                let title_empty = this.title_in.read(cx).value().is_empty();
                return col.pt(if narrow { T::SP_16 } else { T::SP_32 })
                    .child(div().relative().w_full().pl(margin).pt(T::SP_2).pb(T::SP_16).min_h(T::SP_32 + T::SP_16 + T::SP_2)
                        .when(title_empty, |d| d.child(div().absolute().left(margin).top(T::SP_2).text_size(title_fs).font_weight(T::W_BOLD).text_color(T::text_muted()).child("Untitled")))
                        .child(w::textarea(&this.title_in).appearance(false).text_size(title_fs).font_weight(T::W_BOLD).text_color(T::text())))
                    .into_any_element();
            }
            if i == nb + 1 {
                let kt = p.key.clone();
                return col.child(div().id(sid(format!("nt-tail-{}", p.key))).w_full().h(T::NOTES_PAGE_TAIL - if narrow { T::SP_40 } else { px(0.) }).cursor_text()
                    .on_click(move |_, _, cx| super::tail(&kt, cx))).into_any_element();
            }
            let Some(b) = p.blocks.get(i - 1) else { return div().into_any_element() };
            col.child(this.block(p, i - 1, b, margin, k, window, cx)).into_any_element()
        })).size_full();
        let (kb, km, ku) = (key.clone(), key.clone(), key.clone());
        let scroll = div().id(sid(format!("nt-page-{key}"))).size_full()
            .on_mouse_move(move |e: &MouseMoveEvent, _, cx| if e.pressed_button == Some(MouseButton::Left) && super::store(cx).read(cx).press.is_some() { super::blk_over(&km, f32::from(e.position.y), cx); })
            .on_mouse_up(MouseButton::Left, move |_, _, cx| { let _ = &ku; super::store(cx).update(cx, |s, _| s.press = None); })
            .on_drag_move::<BlockDrag>(move |e, _, cx| { if e.drag(cx).key == kb { super::drag_move(f32::from(e.event.position.x), f32::from(e.event.position.y), cx); } })
            .on_drop::<BlockDrag>(|_, _, cx| super::drag_end(true, cx))
            .child(canvas({ let key = key.clone(); move |b, _, cx| super::store(cx).update(cx, |s, _| if let Some(i) = super::idx(s, &key) { s.places[i].page_box = (f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height)); }) }, |_, _, _, _| {}).absolute().size_full())
            .child(page_list);
        // The save status (.nt-bar), top right; its tooltip, the last edit.
        let dot = match p.save_state.as_str() { "busy" => T::yellow(), "err" => T::red(), _ => T::green() };
        let save = w::tip(div().id(sid(format!("nt-save-{key}"))).absolute().right(T::SP_16).top(T::SP_10 + if p.sel_count > 0 { T::SP_40 } else { px(0.) })
            .flex().items_center().gap(T::SP_4).h(T::SP_16)
            .child(div().size(T::SP_6).rounded(T::R_PILL).bg(dot))
            .child(div().text_size(T::TEXT_11 * k).text_color(T::text_muted()).child(p.save_text.clone())), p.save_tip.clone());
        let mut root = div().relative().size_full().overflow_hidden().bg(T::bg())
            .track_focus(&self.sel_focus)
            .child(scroll)
            .child(save);
        // The drop line where a dragged block would land.
        if let Some(y) = p.drop_y {
            let (_, by, bw, _) = { let s = self.store.read(cx); super::idx(s, &key).map(|i| s.places[i].page_box).unwrap_or_default() };
            let max = f32::from(T::NOTES_PAGE_MAX + margin).min(bw);
            let left = (bw - max) / 2. + f32::from(margin) + f32::from(if narrow { T::SP_2 } else { T::SP_16 });
            root = root.child(div().absolute().left(px(left)).w(px(max) - margin - T::SP_32).top(px(y - by) - T::SP_2).h(T::SP_4).rounded(T::R_CHIP).bg(T::accent()));
        }
        // The selection bar (.nt-selbar): over the page, not in it.
        if p.sel_count > 0 {
            let n = p.sel_count;
            let (k1, k2, k3, k4) = (key.clone(), key.clone(), key.clone(), key.clone());
            root = root.child(div().id(sid(format!("nt-selbar-{key}"))).occlude().absolute().top_0().left_0().right_0().flex().items_center().gap(T::SP_4).px(T::SP_12).py(T::SP_6)
                .bg(T::bg_secondary()).border_b_1().border_color(T::border())
                .child(grip(&key, "", "Drag the selection onto a chat or terminal", k, false))
                .child(div().text_size(T::TEXT_12 * k).text_color(T::text_secondary()).child(if n == 1 { "1 block selected".to_string() } else { format!("{n} blocks selected") }))
                .child(div().flex_1())
                .child(w::button(sid(format!("nt-sel-copy-{key}")), "Copy").tooltip("Copy as Markdown (Ctrl+C)").on_click(move |_, _, cx| super::sel_act(&k1, "copy", cx)))
                .child(w::button(sid(format!("nt-sel-all-{key}")), "Select all").tooltip("Select every block (Ctrl+Shift+A)").on_click(move |_, _, cx| super::sel_act(&k2, "all", cx)))
                .child(w::danger(sid(format!("nt-sel-del-{key}")), "Delete").tooltip("Delete the selected blocks").on_click(move |_, _, cx| super::sel_act(&k3, "delete", cx)))
                .child(w::icon_button(sid(format!("nt-sel-x-{key}")), IconName::X, "Clear the selection (Esc)").on_click(move |_, _, cx| super::sel_act(&k4, "clear", cx))));
        }
        root
    }

    /// One block (.nt-block): its margin (+ and the grip on hover) and itself.
    #[allow(clippy::too_many_arguments)]
    fn block(&mut self, p: &PlaceRow, ix: usize, r: &BlockRow, margin: Pixels, k: f32, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        crundi_core::driver::rendered("notes-block");
        let b = &r.b;
        let key = p.key.clone();
        let group: SharedString = format!("nt-blk-{key}-{}", b.id).into();
        let fs = match b.kind.as_str() { "h1" => (T::TEXT_20 + T::SP_2) * k, "h2" => (T::TEXT_16 + T::SP_2) * k, "h3" => T::TEXT_16 * k, _ => T::TEXT_15 * k };
        let top = match b.kind.as_str() { "h1" => T::SP_16, "h2" => T::SP_12, "h3" => T::SP_8, _ => px(0.) };
        let (kp, bp) = (key.clone(), b.id.clone());
        let margin_area = div().absolute().left_0().top_0().bottom_0().w(margin)
            .on_mouse_down(MouseButton::Left, move |e, _, cx| super::blk_press(&kp, &bp, e.modifiers.shift, cx));
        let (ka, ba) = (key.clone(), b.id.clone());
        let tools = div().absolute().left(margin - T::SP_40 - T::SP_2).top(top + T::SP_2).w(T::SP_40).h(T::SP_24).flex().gap(px(1.))
            .when(!r.menu_open, |d| d.invisible().group_hover(group.clone(), |s| s.visible()))
            .child(w::square_button(sid(format!("nt-add-{key}-{}", b.id)), IconName::Plus, "Add a block below", w::SquareLook::Bare, T::SP_20, T::ICON_MD * k)
                .on_click(move |_, _, cx| super::add_below(&ka, &ba, cx)))
            .child(grip(&key, &b.id, "Click: turn into, move, duplicate, delete. Drag: onto a chat or terminal to send it as Markdown", k, true));
        let lh = fs * T::LH_CHAT;
        let mut body = div().flex().w_full().gap(T::SP_6).py(px(1.));
        match b.kind.as_str() {
            "todo" => {
                let (kc, bc, on) = (key.clone(), b.id.clone(), b.checked);
                body = body.child(div().flex_none().pl(T::SP_2).pr(T::SP_4).pt((lh - T::SP_16) / 2.)
                    .child(div().id(sid(format!("nt-chk-{key}-{}", b.id))).size(T::SP_16).rounded(T::R_CHIP).border_1().cursor_pointer()
                        .border_color(if on { T::accent() } else { T::composer_slate_edge() }).bg(if on { T::accent() } else { T::bg() })
                        .hover(|s| s.border_color(T::accent_hover()))
                        .flex().items_center().justify_center().text_color(T::white()).text_size(T::ICON_SM)
                        .when(on, |d| d.child(IconName::Check))
                        .on_click(move |_, _, cx| super::edit_block(&kc, &bc, cx, |b| b.checked = !on))));
            }
            "bullet" => { body = body.child(div().flex_none().w(T::SP_20 + T::SP_2).pt((lh - T::SP_6) / 2. + px(1.)).child(div().size(T::SP_6).rounded(T::R_PILL).bg(T::text_secondary()))); }
            "number" => { body = body.child(div().flex_none().w(T::SP_20 + T::SP_2).pr(T::SP_2).h(lh).flex().items_center().justify_end().text_size(fs).text_color(T::text_secondary()).child(r.num.clone())); }
            "quote" => { body = body.child(div().flex_none().w(T::SP_2 + px(1.)).bg(T::border())).child(div().flex_none().w(px(0.))); }
            _ => {}
        }
        let content = match b.kind.as_str() {
            "code" => self.code_block(&key, b, k, group.clone()).into_any_element(),
            "table" => self.table_block(p, b, k).into_any_element(),
            "divider" => self.divider_block(&key, b, window).into_any_element(),
            _ => {
                let weight = match b.kind.as_str() { "h1" => T::W_BOLD, "h2" | "h3" => T::W_SEMIBOLD, _ => T::W_REGULAR };
                let ink = if b.kind == "quote" { T::text_secondary() } else if b.kind == "todo" && b.checked { T::text_muted() } else { T::text() };
                self.text_cell(&key, &b.id, -1, -1, &b.text, doc::placeholder(&b.kind), fs, weight, ink, b.kind == "todo" && b.checked, false, false).into_any_element()
            }
        };
        let _ = cx;
        body = body.child(div().flex_1().min_w_0().child(content));
        let (kbx, bbx) = (key.clone(), b.id.clone());
        div().id(sid(format!("nt-b-{key}-{ix}"))).group(group).relative().w_full().pt(top)
            .child(canvas(move |bd, _, cx| super::store(cx).update(cx, |s, _| if let Some(i) = super::idx(s, &kbx) {
                let bx = &mut s.places[i].boxes;
                bx.retain(|x| x.0 != bbx);
                bx.push((bbx.clone(), f32::from(bd.origin.y), f32::from(bd.size.height)));
            }), |_, _, _, _| {}).absolute().size_full())
            .when(r.sel, |d| d.child(div().absolute().left(margin - T::SP_6).right_0().top_0().bottom_0().bg(T::accent_dim())
                .when(r.sel_top, |d| d.rounded_t(T::R_CONTROL)).when(r.sel_bottom, |d| d.rounded_b(T::R_CONTROL))))
            .child(margin_area)
            .child(tools)
            .child(div().pl(margin).child(body))
    }

    /// A block's (or cell's) text: the text box while it has the caret, the
    /// formatted text otherwise (a click puts the caret at its end; with
    /// Ctrl, a click on a link opens it, the web's).
    #[allow(clippy::too_many_arguments)]
    fn text_cell(&mut self, key: &str, id: &str, r: i32, c: i32, src: &str, ph: &str, fs: Pixels, weight: FontWeight, ink: Hsla, done: bool, mono: bool, nowrap: bool) -> AnyElement {
        let here = self.editing && self.edit.as_ref() == Some(&(id.to_string(), r, c));
        if here {
            let mut d = div().relative().w_full();
            if src.is_empty() && !ph.is_empty() {
                d = d.child(div().absolute().left_0().top_0().text_size(fs).line_height(relative(T::LH_CHAT)).font_weight(weight).text_color(T::text_muted()).child(ph.to_string()));
            }
            let t = w::textarea(&self.edit_in).appearance(false).text_size(fs).font_weight(weight).text_color(ink).line_height(relative(T::LH_CHAT));
            return d.child(if mono { t.font_family(T::MONO) } else { t }).into_any_element();
        }
        // The parse, once per source.
        let ck = format!("{id}-{r}-{c}");
        let rich = match self.rich.get(&ck) {
            Some((s, d, x)) if s == src && *d == done => x.clone(),
            _ => { let x = Rc::new(parse_rich(src, done)); self.rich.insert(ck, (src.to_string(), done, x.clone())); x }
        };
        let styled = StyledText::new(rich.text.clone()).with_highlights(rich.hl.clone());
        let layout = styled.layout().clone();
        let (k, i) = (key.to_string(), id.to_string());
        let n = src.len();
        div().id(sid(format!("nt-t-{key}-{id}-{r}-{c}"))).w_full().cursor_text().font_weight(weight).text_color(ink)
            .on_click(move |e, window, cx| {
                let m = window.modifiers();
                if (m.control || m.platform) && !rich.links.is_empty() {
                    if let Ok(ix) = layout.index_for_position(e.position()) {
                        if let Some(l) = rich.links.iter().find(|l| l.0.contains(&ix)) { super::open_link(&l.1, cx); }
                    }
                }
                super::request_focus(&k, &i, r, c, n, n, cx);
            })
            .child(div().w_full().text_size(fs).line_height(relative(T::LH_CHAT)).when(mono, |d| d.font_family(T::MONO)).when(nowrap, |d| d.whitespace_nowrap()).child(styled))
            .into_any_element()
    }

    /// A code block (.nt-code-wrap): the language, wrap and copy above it while
    /// hovered or edited; the code (mono).
    fn code_block(&mut self, key: &str, b: &doc::Block, k: f32, group: SharedString) -> Div {
        let editing = self.editing && self.edit.as_ref().is_some_and(|e| e.0 == b.id);
        let lang = doc::lang_name(&doc::norm_lang(&b.lang));
        let (k1, b1, k2, b2, k3, b3) = (key.to_string(), b.id.clone(), key.to_string(), b.id.clone(), key.to_string(), b.id.clone());
        let head = div().absolute().right(T::SP_6).top(px(0.)).flex().gap(T::SP_2).p(T::SP_2).bg(T::bg_secondary()).rounded_t(T::R_CONTROL).border_1().border_color(T::border_subtle())
            .when(!editing, |d| d.invisible().group_hover(group, |s| s.visible()))
            .child(w::head_button(sid(format!("nt-lang-{key}-{}", b.id)), C::IconText(IconName::CodeXml, lang.into()), L::Rest, T::SP_48 * 2., T::SP_20 + T::SP_2, "Code language")
                .on_click(move |e, window, cx| { let p = e.position(); let p = if p.x > px(0.) { p } else { window.mouse_position() }; super::code_act(&k1, &b1, "lang", f32::from(p.x) + f32::from(T::SP_24), f32::from(p.y) + f32::from(T::SP_12), cx); }))
            .child(w::head_button(sid(format!("nt-wrap-{key}-{}", b.id)), C::Icon(IconName::TextWrap), if b.wrap { L::Lit } else { L::Rest }, T::SP_24, T::SP_20 + T::SP_2,
                if b.wrap { "Wrapping long lines (click to scroll instead)" } else { "Wrap long lines" })
                .on_click(move |_, _, cx| super::code_act(&k2, &b2, "wrap", 0., 0., cx)))
            .child(w::head_button(sid(format!("nt-copy-{key}-{}", b.id)), C::Icon(IconName::Copy), L::Rest, T::SP_24, T::SP_20 + T::SP_2, "Copy code")
                .on_click(move |_, _, cx| super::code_act(&k3, &b3, "copy", 0., 0., cx)));
        let code = self.text_cell(key, &b.id, -1, -1, &b.text, "Code", T::TEXT_12 * k, T::W_REGULAR, T::text(), false, true, !b.wrap);
        div().relative().w_full().pt(T::SP_24 + T::SP_4).pb(T::SP_4)
            .child(div().id(sid(format!("nt-code-{key}-{}", b.id))).w_full().p(T::SP_12).rounded(T::R_FIELD).bg(T::bg_secondary()).border_1()
                .border_color(if editing { T::accent_edge() } else { T::border_subtle() })
                .when(!b.wrap && !editing, |d| d.overflow_x_scroll())
                .child(code))
            .child(head)
    }

    /// A table (.nt-table): equal columns, the first row bold on a tint when
    /// it is a header; its buttons under it while a cell has the caret.
    fn table_block(&mut self, p: &PlaceRow, b: &doc::Block, k: f32) -> Div {
        let key = p.key.clone();
        let rows = b.rows.len();
        let mut grid = div().flex().flex_col().w_full().border_1().border_color(T::border());
        for (ri, row) in b.rows.iter().enumerate() {
            let mut line = div().flex().w_full().when(ri + 1 < rows, |d| d.border_b_1().border_color(T::border()));
            let cols = row.len();
            for (ci, cell) in row.iter().enumerate() {
                let head = b.header && ri == 0;
                let here = self.editing && self.edit.as_ref() == Some(&(b.id.clone(), ri as i32, ci as i32));
                let t = self.text_cell(&key, &b.id, ri as i32, ci as i32, cell, "", T::TEXT_15 * k, if head { T::W_SEMIBOLD } else { T::W_REGULAR }, T::text(), false, false, false);
                line = line.child(div().flex_1().min_w(T::SP_40).px(T::SP_8).py(T::SP_6)
                    .when(head, |d| d.bg(T::bg_secondary()))
                    .when(ci + 1 < cols, |d| d.border_r_1().border_color(T::border()))
                    .when(here, |d| d.border_1().border_color(T::accent_edge()))
                    .child(t));
            }
            grid = grid.child(line);
        }
        let mut col = div().flex().flex_col().w_full().py(T::SP_6).gap(T::SP_4).child(grid);
        if p.cur_id == b.id {
            let mut bar = div().flex().gap(T::SP_4);
            for (a, label) in [("add-row", "+ Row"), ("add-col", "+ Column"), ("del-row", "\u{2212} Row"), ("del-col", "\u{2212} Column"), ("header", "Header row")] {
                let (kk, bb) = (key.clone(), b.id.clone());
                // A press here keeps the cell's focus (the web's mousedown preventDefault).
                bar = bar.child(div().on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .child(w::button(sid(format!("nt-tb-{a}-{key}-{}", b.id)), label).on_click(move |_, _, cx| super::table_act(&kk, &bb, a, cx))));
            }
            col = col.child(bar);
        }
        col
    }

    /// A divider (.nt-hr): a line that takes the focus (accent then).
    fn divider_block(&self, key: &str, b: &doc::Block, window: &mut Window) -> Stateful<Div> {
        let on = self.edit.as_ref().is_some_and(|e| e.0 == b.id && e.1 == -2) && self.div_focus.is_focused(window);
        let (k, i) = (key.to_string(), b.id.clone());
        let mut d = div().id(sid(format!("nt-hr-{key}-{}", b.id))).w_full().h(T::SP_12 + T::SP_16 + px(1.)).pt(T::SP_12 + T::SP_2).cursor_pointer()
            .on_click(move |_, _, cx| super::request_focus(&k, &i, -1, -1, 0, 0, cx))
            .child(div().w_full().h(px(1.)).bg(if on { T::accent() } else { T::border() }));
        if self.edit.as_ref().is_some_and(|e| e.0 == b.id && e.1 == -2) { d = d.track_focus(&self.div_focus); }
        d
    }
}

/// The grip (.nt-grip): click for the block's menu, drag to move it.
fn grip(key: &str, bid: &str, tip: &str, k: f32, click: bool) -> Stateful<Div> {
    let (k1, b1, k2, b2) = (key.to_string(), bid.to_string(), key.to_string(), bid.to_string());
    let g = w::square_button(sid(format!("nt-grip-{key}-{bid}")), IconName::GripVertical, tip.to_string(), w::SquareLook::Bare, T::SP_20, T::ICON_MD * k)
        .cursor_grab()
        .on_drag(BlockDrag { key: key.to_string(), label: String::new() }, move |_, _, _, cx| {
            let label = super::drag_start(&k1, &b1, cx).map(|d| d.label).unwrap_or_default();
            cx.new(|_| DragGhost(label))
        });
    if click { g.on_click(move |e, window, cx| { let p = e.position(); let p = if p.x > px(0.) { p } else { window.mouse_position() }; super::block_menu(&k2, &b2, f32::from(p.x), f32::from(p.y) + f32::from(T::SP_12), cx); }) } else { g }
}

/// What a grip's drag carries (GPUI's drag and drop).
#[derive(Clone)]
pub(crate) struct BlockDrag { pub key: String, pub label: String }

// ─── What floats over the page: the menus, the format bar ───

impl PlaceView {
    fn floats(&self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (mine, menu, fmt) = { let s = self.store.read(cx); (s.float.as_ref().is_some_and(|f| f.key() == self.key), s.menu.clone(), s.fmt.clone()) };
        if !mine { return vec![]; }
        let mut out = vec![];
        let win = window.viewport_size();
        if menu.open {
            let x = menu.x.max(f32::from(T::SP_8)).min(f32::from(win.width - T::NOTES_MENU_W - T::SP_8));
            let mut panel = w::menu_panel("nt-menu").w(T::NOTES_MENU_W).max_h(T::NOTES_MENU_MAX_H).overflow_y_scroll().p(T::SP_4).gap(px(0.))
                .on_mouse_down_out(|_, _, cx| super::close_float(cx));
            if menu.query_on { panel = panel.child(div().pb(T::SP_6).child(w::input(&self.query_in))); }
            if !menu.label.is_empty() { panel = panel.child(div().px(T::SP_8).pt(T::SP_6).pb(T::SP_2).text_size(T::TEXT_11).text_color(T::text_muted()).child(menu.label.clone())); }
            if menu.items.is_empty() && !menu.empty.is_empty() {
                panel = panel.child(div().py(T::SP_16).text_center().text_size(T::TEXT_12).text_color(T::text_muted()).child(menu.empty.clone()));
            }
            for (i, it) in menu.items.iter().enumerate() {
                if it.sep { panel = panel.child(div().py(T::SP_4).child(w::rule())); }
                panel = panel.child(w::key_row(sid(format!("nt-mi-{i}")), it.k.clone(), it.name.clone(), it.d.clone(), it.check, it.danger, i as i32 == menu.act)
                    .on_click(move |_, _, cx| super::menu_pick(i as i32, cx)));
            }
            out.push(deferred(anchored().position(point(px(x), px(menu.y))).snap_to_window_with_margin(T::SP_8).child(panel)).with_priority(2).into_any_element());
        }
        if fmt.open {
            let mut bar = div().id("nt-fmt").occlude().flex().items_center().gap(T::SP_2).p(T::SP_4).bg(T::bg_secondary()).border_1().border_color(T::border()).rounded(T::R_FIELD).shadow_lg();
            if !fmt.link {
                bar = bar.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .child(grip("__fmt", "", "Drag the selected text onto a chat or terminal", 1., false));
                for (a, label, tip) in [("bold", "B", "Bold (Ctrl+B)"), ("italic", "I", "Italic (Ctrl+I)"), ("underline", "U", "Underline (Ctrl+U)"), ("strike", "S", "Strikethrough (Ctrl+Shift+S)"), ("code", "</>", "Code (Ctrl+E)"), ("link", "Link", "Link (Ctrl+K)")] {
                    bar = bar.child(w::ghost(sid(format!("nt-fmt-{a}")), label).tooltip(tip).on_click(move |_, _, cx| super::fmt_act(a, cx)));
                }
                bar = bar.child(div().w(px(1.)).h(T::SP_20).bg(T::border()));
                bar = bar.child(w::ghost("nt-fmt-copy", "Copy").tooltip("Copy as Markdown").on_click(|_, _, cx| super::fmt_act("copy", cx)));
                bar = bar.child(w::ghost("nt-fmt-all", "Select all").tooltip("Select every block (Ctrl+Shift+A)").on_click(|_, _, cx| super::fmt_act("all", cx)));
            } else {
                bar = bar.on_mouse_down_out(|_, _, cx| super::link_cancel(cx))
                    .child(div().w(T::NOTES_LINK_W).child(w::input(&self.link_in)));
                if fmt.has_link { bar = bar.child(w::ghost("nt-link-rm", "Remove").tooltip("Remove the link").on_click(|_, _, cx| super::link_apply("", cx))); }
            }
            let y = (fmt.y - f32::from(T::SP_40 + T::SP_8)).max(f32::from(T::SP_8));
            let x = (fmt.x - f32::from(T::NOTES_MENU_W)).max(f32::from(T::SP_8));
            out.push(deferred(anchored().position(point(px(x), px(y))).snap_to_window_with_margin(T::SP_8).child(bar)).with_priority(2).into_any_element());
        }
        out
    }
}

/// A block being dragged: its label by the pointer.
struct DragGhost(String);

impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().ml(T::SP_12).mt(T::SP_8).max_w(T::NOTES_MENU_W + T::SP_40).px(T::SP_10).py(T::SP_6).rounded(T::R_CONTROL)
            .bg(T::bg_secondary()).border_1().border_color(T::accent()).opacity(T::NOTES_O_CLONE)
            .text_size(T::TEXT_12).text_color(T::text()).truncate().child(self.0.clone())
    }
}
