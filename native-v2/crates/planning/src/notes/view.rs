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
//! One text box per place serves whichever block (or table cell) has the
//! caret. It holds the block's words, formatted as they are at rest (text
//! decorations from the runs; a code block's colours from its language), and
//! each change of the words is diffed into the runs (doc::edit_runs): the
//! model keeps the source (doc.rs), the box never shows it. Offsets cross
//! between the two with doc::box_to_src / src_to_box.

use super::{doc, BlockRow, Item, PlaceRow, S};
use crundi_theme::{IconName, T};
use crundi_widgets::{self as w, HeadContent as C, HeadLook as L, InputEvent, InputState, TextareaState};
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::Sizable as _;
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
    cx.new(|cx| {
        // Whether a project is open, copied when the store changes it: the
        // tab is a layout view and never reads the store while rendering (a
        // store notify that changes nothing it lays out draws no frame).
        let sub = cx.observe(&st, |this: &mut TabView, s, cx| { let p = s.read(cx).tab_project; if p != this.project { this.project = p; cx.notify(); } });
        let project = st.read(cx).tab_project;
        TabView { place, side, bar, width: 0., project, _sub: sub }
    })
}

/// The Notes tab: a layout (no text of its own): the bar over the list and
/// the page, side by side (stacked when narrow).
pub(crate) struct TabView {
    place: Entity<PlaceView>,
    side: Entity<SideView>,
    bar: Entity<TabBar>,
    width: f32,
    project: bool,
    _sub: Subscription,
}

impl Render for TabView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("notes-tab");
        let project = self.project;
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

/// A code block's text as it is, coloured by its language (`core::syntax`,
/// the theme's syn_* colours, as the chat colours code).
fn parse_code(src: &str, lang: &str) -> Rich {
    use crundi_core::syntax::{highlight, Syn};
    let hl = highlight(lang, src).into_iter().map(|(r, k)| {
        let c = match k {
            Syn::Comment => T::syn_comment(), Syn::String => T::syn_string(), Syn::Number => T::syn_number(), Syn::Keyword => T::syn_keyword(),
            Syn::Type => T::syn_type(), Syn::Function => T::syn_function(), Syn::Tag => T::syn_keyword(), Syn::Attribute => T::syn_number(),
        };
        (r, HighlightStyle { color: Some(c), ..Default::default() })
    }).collect();
    let text: SharedString = if src.is_empty() { "\u{a0}".into() } else { src.to_string().into() };
    Rich { text, hl, links: vec![] }
}

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

/// The box's formatting for a block's source: the runs' styles, or a code
/// block's colours (`lang`: the highlighter's language).
fn decorations(src: &str, lang: Option<&str>, done: bool) -> Vec<gpui_component::input::TextDecoration> {
    if src.is_empty() { return vec![]; }
    let rich = match lang { Some(l) => parse_code(src, l), None => parse_rich(src, done) };
    rich.hl.iter().map(|(r, h)| gpui_component::input::TextDecoration::new(r.clone(), *h)).collect()
}

/// A code block's language for the highlighter.
fn code_lang(b: &doc::Block) -> String {
    let hl = doc::hl_name(&doc::norm_lang(&b.lang));
    if hl == "Dockerfile" { return hl; }
    hl.rsplit('.').next().unwrap_or("").to_string()
}

/// One place: the picker, the page, or a line; in a pane, its tools' row.
pub(crate) struct PlaceView {
    key: String,
    store: Entity<S>,
    /// The store's drag-select, as this frame draws it.
    tsel: Option<super::TSel>,
    dspot: Option<super::DropSpot>,
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
    /// The open menu's scroll, and the item last scrolled into view (its
    /// generation, the active item): a menu opens at its current item.
    menu_scroll: ScrollHandle,
    menu_shown: std::cell::Cell<(u64, i32)>,
    /// The format bar's size as last laid out (it centres on the selection).
    fmt_size: Rc<std::cell::Cell<(f32, f32)>>,
    /// The block (row, column for a cell; -2 a divider) the text box serves.
    edit: Option<(String, i32, i32)>,
    /// The source the box holds the words of; a code block's language (the
    /// box holds its code as it is); a ticked to-do (struck through).
    edit_src: String,
    edit_lang: Option<String>,
    edit_done: bool,
    /// T13: the box's soft wrap (off while a code block that scrolls sideways has the caret).
    edit_wrap: bool,
    /// T13: each code block's sideways scroll (the driver reads it).
    code_scroll: HashMap<String, ScrollHandle>,
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
                        // The words changed: into the runs (typed text takes
                        // its neighbour's style), the box's formatting again.
                        let (t, caret) = { let s = st.read(cx); (s.value().to_string(), s.cursor()) };
                        let code = this.edit_lang.is_some();
                        let src = if code { t } else { doc::runs_to_src(&doc::edit_runs(&doc::src_to_runs(&this.edit_src), &t)) };
                        if src == this.edit_src { return; }
                        this.edit_src = src.clone();
                        let decos = decorations(&src, this.edit_lang.as_deref(), this.edit_done);
                        st.update(cx, |s, cx| s.set_decorations(decos, cx));
                        let at = if code { caret } else { doc::box_to_src(&src, caret) };
                        if r >= 0 { super::cell_edited(&k, &id, r, c, &src, cx); } else { super::edited(&k, &id, &src, at as i32, cx); }
                    }
                    InputEvent::Focus => { this.editing = true; super::focused(&k, &id, r, c, cx); }
                    InputEvent::Blur => { this.editing = false; super::blur(&k, &id, cx); cx.notify(); }
                    _ => {}
                }
            }),
            cx.observe(&edit_in, |this, _, cx| {
                // The selection moved: the format bar follows it (sel_moved).
                if !this.editing { return; }
                this.report_sel(false, cx);
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
            applied_focus: 0, applied_title: -1, menu_gen: 0, fmt_gen: 0, menu_scroll: ScrollHandle::new(), menu_shown: std::cell::Cell::new((u64::MAX, -1)), fmt_size: Rc::new(std::cell::Cell::new((0., 0.))), edit: None, edit_src: String::new(), edit_lang: None, edit_done: false, edit_wrap: true, code_scroll: HashMap::new(), editing: false, last_sel: (0, 0), width: 0.,
            page: ListState::new(0, ListAlignment::Top, T::SP_48 * 8.), page_sigs: vec![], pick: ListState::new(0, ListAlignment::Top, T::SP_48 * 4.), pick_sigs: vec![],
            mode: String::new(), shown_page: String::new(), rich: HashMap::new(), tsel: None, dspot: None, _subs: subs,
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
                    let was = self.edit.replace((id.clone(), r, c));
                    let same = was.as_ref() == Some(&(id.clone(), r, c)) && self.editing;
                    self.edit_lang = (r < 0 && row.b.kind == "code").then(|| code_lang(&row.b));
                    self.edit_done = r < 0 && row.b.kind == "todo" && row.b.checked;
                    self.edit_src = src;
                    let (a, b) = (self.to_box(a), self.to_box(b));
                    self.last_sel = (a, b);
                    self.load_box(Some(a..b), window, cx);
                    self.edit_in.update(cx, |st, cx| st.focus(window, cx));
                    if a != b && p.focus_fmt {
                        // A held drag-selection handed to the box (F187): the
                        // web's mouseup runs maybeFormatBar. The box starts with
                        // the range, so its observer sees no move; once it has
                        // drawn the words (range_to_bounds), the bar opens over them.
                        let me = cx.entity().downgrade();
                        window.on_next_frame(move |_, cx| { let _ = me.update(cx, |this, cx| if this.editing && this.last_sel == (a, b) { this.report_sel(true, cx) }); });
                    }
                    if same {
                        // Still the same block: tell the model as a fresh focus would.
                        let k = self.key.clone();
                        cx.defer(move |cx| super::focused(&k, &id, r, c, cx));
                    }
                    self.editing = true;
                }
            }
        }
        // The block with the caret changed under it (another client's edit,
        // the checkbox): the box takes its words and formatting again.
        if let Some((id, r, c)) = self.edit.clone().filter(|e| e.1 > -2 && self.editing) {
            if let Some(row) = p.blocks.iter().find(|x| x.b.id == id) {
                let src = if r >= 0 { row.b.rows.get(r as usize).and_then(|x| x.get(c as usize)).cloned().unwrap_or_default() } else { row.b.text.clone() };
                let done = r < 0 && row.b.kind == "todo" && row.b.checked;
                if src != self.edit_src || done != self.edit_done {
                    self.edit_src = src;
                    self.edit_done = done;
                    self.load_box(None, window, cx);
                }
            }
        }
        // T13: a code block set to scroll (not wrap) keeps its long lines
        // while edited: the box doesn't wrap them, it scrolls sideways to
        // the caret (the web's pre in .nt-code-wrap).
        let want = self.edit.as_ref().filter(|e| self.editing && e.1 == -1)
            .and_then(|(id, _, _)| p.blocks.iter().find(|x| &x.b.id == id))
            .is_none_or(|row| row.b.kind != "code" || row.b.wrap);
        if want != self.edit_wrap {
            self.edit_wrap = want;
            self.edit_in.update(cx, |st, cx| st.set_soft_wrap(want, window, cx));
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

    /// For the driver: the block the box serves, its words, how many
    /// formatted stretches it draws them with, the caret (box offsets).
    pub(crate) fn edit_state(&self, cx: &App) -> serde_json::Value {
        let st = self.edit_in.read(cx);
        let r = st.selected_range();
        serde_json::json!({ "id": self.edit.as_ref().map(|e| e.0.clone()).unwrap_or_default(), "on": self.editing, "text": st.value().to_string(),
            "decos": st.decorations().len(), "bold": st.decorations().iter().filter(|d| d.style.font_weight.is_some()).count(), "colours": st.decorations().iter().filter(|d| d.style.color.is_some()).count(),
            "a": r.start, "b": r.end, "softWrap": self.edit_wrap,
            "caretX": st.cursor_layout().map(|(b, _)| f32::from(b.origin.x)),
            "code": self.code_scroll.iter().map(|(id, h)| (id.clone(), serde_json::json!({ "x": -f32::from(h.offset().x), "max": f32::from(h.max_offset().x),
                "left": f32::from(h.bounds().origin.x), "w": f32::from(h.bounds().size.width) }))).collect::<serde_json::Map<_, _>>() })
    }

    /// A byte offset of the box's words as an offset of the block's source.
    /// The box's selection to the model (sel_moved: the format bar follows
    /// it). `force`: even when the range is the one last seen (a held
    /// drag-selection handed to the box starts with its range, F187).
    fn report_sel(&mut self, force: bool, cx: &mut Context<Self>) {
        let Some((id, _, _)) = self.edit.clone() else { return };
        let (range, rev, caret, span) = { let s = self.edit_in.read(cx); let r = s.selected_range(); let span = (r.start != r.end).then(|| s.range_to_bounds(&r)).flatten(); (r, s.is_selection_reversed(), s.cursor_layout(), span) };
        let sel = (range.start, range.end);
        if sel == self.last_sel && !force { return; }
        self.last_sel = sel;
        let (a, c) = if rev { (range.end, range.start) } else { (range.start, range.end) };
        let (a, c) = (self.to_src(a), self.to_src(c));
        let (x, y) = caret.map(|(b, _)| (f32::from(b.origin.x), f32::from(b.origin.y))).unwrap_or_else(|| self.store.read(cx).caret_xy);
        let k = self.key.clone();
        // The selection's own ends (a double-click selects a word in
        // one go: no caret was seen before it to anchor the bar).
        let span = span.map(|b| (f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.origin.x + b.size.width)));
        cx.defer(move |cx| super::sel_moved(&k, &id, a as i32, c as i32, x, y, span, cx));
    }

    fn to_src(&self, at: usize) -> usize { if self.edit_lang.is_some() { at.min(self.edit_src.len()) } else { doc::box_to_src(&self.edit_src, at) } }
    /// A source offset as an offset of the box's words.
    fn to_box(&self, at: usize) -> usize { if self.edit_lang.is_some() { at.min(self.edit_src.len()) } else { doc::src_to_box(&self.edit_src, at) } }

    /// The box takes edit_src's words and formatting; the selection given
    /// (box offsets), or the one it had, kept inside the words.
    fn load_box(&mut self, sel: Option<std::ops::Range<usize>>, window: &mut Window, cx: &mut Context<Self>) {
        let text = if self.edit_lang.is_some() { self.edit_src.clone() } else { doc::src_plain(&self.edit_src) };
        let decos = decorations(&self.edit_src, self.edit_lang.as_deref(), self.edit_done);
        self.edit_in.update(cx, |st, cx| {
            let sel = sel.unwrap_or_else(|| st.selected_range());
            let n = text.len();
            if st.value().as_ref() != text.as_str() { st.set_value(text, window, cx); }
            st.set_decorations(decos, cx);
            st.set_selected_range(sel.start.min(n)..sel.end.min(n), cx);
        });
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
            let (text, cc_box) = (text, cc);
            let (a, cc) = (self.to_src(a), self.to_src(cc));
            if r >= 0 { return Some(super::cell_key(&k, &id, r, c, name, a as i32, cc as i32, cx)); }
            // On the first or last line (visually, from the caret's box; by
            // line breaks before the first layout).
            let (first, last) = match caret_layout {
                Some((b, lh)) if ib.size.height > px(0.) => (b.top() < ib.top() + lh, b.bottom() > ib.bottom() - lh),
                _ => (!text[..cc_box.min(text.len())].contains('\n'), !text[cc_box.min(text.len())..].contains('\n')),
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
    // Alt only moves blocks (Alt+Up / Alt+Down).
    if m.alt { return if !m.control && !m.platform && !m.shift && matches!(k.key.as_str(), "up" | "down") { format!("alt-{}", k.key) } else { String::new() }; }
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
        // Anything held near the page's top or bottom scrolls it (the web's wbAutoScroll).
        let scroll = w::edge_scroll::<w::RefDrag>(div().id(sid(format!("nt-page-{key}"))), w::Scroller::List(self.page.clone()), false, true, cx.entity_id()).size_full()
            .on_mouse_move(move |e: &MouseMoveEvent, _, cx| if e.pressed_button == Some(MouseButton::Left) && !cx.has_active_drag() && super::store(cx).read(cx).press.is_some() { super::blk_over(&km, f32::from(e.position.y), cx); })
            .on_mouse_up(MouseButton::Left, move |_, _, cx| { super::store(cx).update(cx, |s, _| s.press = None); let _ = super::tsel_end(&ku, cx); })
            // Blocks of this page in hand: where they would land; let go: they move.
            .on_drag_move::<w::RefDrag>(move |e, _, cx| { if e.drag(cx).kind == "note" && super::drag_key(cx).as_deref() == Some(kb.as_str()) { super::drag_move(f32::from(e.event.position.x), f32::from(e.event.position.y), cx); } })
            // Anything else (another panel's reference, another page's
            // blocks) goes in as text under the pointer (the web's dropText).
            .on_drop::<w::RefDrag>({ let kd = key.clone(); move |d, window, cx| {
                if d.kind == "note" && super::drag_key(cx).as_deref() == Some(kd.as_str()) { super::drag_end(true, cx); }
                else { let _ = window; super::drop_text(&kd, &d.plain(), cx); }
                cx.stop_propagation();
            } })
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
                .child(grip(&key, "", "Drag the selection onto a chat or terminal", k, false, cx))
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
        self.tsel = self.store.read(cx).tsel.clone();
        self.dspot = if cx.has_active_drag() { self.store.read(cx).dspot.clone() } else { None };
        let b = &r.b;
        let key = p.key.clone();
        let group: SharedString = format!("nt-blk-{key}-{}", b.id).into();
        let fs = match b.kind.as_str() { "h1" => (T::TEXT_20 + T::SP_2) * k, "h2" => (T::TEXT_16 + T::SP_2) * k, "h3" => T::TEXT_16 * k, _ => T::TEXT_15 * k };
        let top = match b.kind.as_str() { "h1" => T::SP_16, "h2" => T::SP_12, "h3" => T::SP_8, _ => px(0.) };
        let (kp, bp) = (key.clone(), b.id.clone());
        // A press anywhere on the block (its margin or its words) may start a
        // block selection: a drag into the next block selects blocks, and
        // Shift with a press on another block selects from the one with the
        // caret (the web's). Inside one block, the words select as text.
        let margin_area = div().absolute().left_0().top_0().bottom_0().w(margin);
        let (ka, ba) = (key.clone(), b.id.clone());
        let (kpl, bpl) = (key.clone(), b.id.clone());
        // Where its "+" is (the driver clicks the real one).
        let plus_at = canvas(move |bd, _, cx| super::store(cx).update(cx, |s, _| if let Some(i) = super::idx(s, &kpl) {
            let pl = &mut s.places[i].plus;
            pl.retain(|x| x.0 != bpl);
            pl.push((bpl.clone(), f32::from(bd.origin.x) + f32::from(T::SP_20) / 2., f32::from(bd.origin.y) + f32::from(bd.size.height) / 2.));
        }), |_, _, _, _| {}).absolute().left_0().top_0().w(T::SP_20).h_full();
        // Shown while the block is hovered, as the web's .nt-gutter (opacity
        // 0, not hidden): a press on the grip's spot takes the grip even
        // before a frame has drawn it, so its drag starts on the first move
        // (F258: hidden, it had no hitbox until the hover's frame landed,
        // and the press went to the margin).
        let tools = div().absolute().left(margin - T::SP_40 - T::SP_2).top(top + T::SP_2).w(T::SP_40).h(T::SP_24).flex().gap(px(1.))
            .when(!r.menu_open, |d| d.opacity(0.).group_hover(group.clone(), |s| s.opacity(1.)))
            .child(plus_at)
            .child(w::square_button(sid(format!("nt-add-{key}-{}", b.id)), IconName::Plus, "Add a block below", w::SquareLook::Bare, T::SP_20, T::ICON_MD * k)
                .on_click(move |e, window, cx| { let p = e.position(); let p = if p.x > px(0.) { p } else { window.mouse_position() }; super::add_below(&ka, &ba, Some((f32::from(p.x), f32::from(p.y))), cx) }))
            .child(grip(&key, &b.id, "Click: turn into, move, duplicate, delete. Drag: onto a chat or terminal to send it as Markdown", k, true, cx));
        // The fold (.nt-fold): left of the "+" and the grip, on a block with
        // children; shown on hover, and always while folded.
        let fold = r.kids.then(|| {
            let (kf, bf) = (key.clone(), b.id.clone());
            // Borderless and muted, as the web's gutter buttons; hover lights it.
            div().id(sid(format!("nt-fold-{key}-{}", b.id))).absolute().left(margin - T::SP_40 - T::SP_2 - T::SP_14).top(top + T::SP_2).w(T::SP_14).h(T::SP_22)
                .flex().items_center().justify_center().rounded(T::R_CHIP).cursor_pointer().text_color(T::text_muted())
                .hover(|s| s.bg(T::bg_hover()).text_color(T::text()))
                .when(!b.folded && !r.menu_open, |d| d.opacity(0.).group_hover(group.clone(), |s| s.opacity(1.)))
                .child(w::icon(if b.folded { IconName::ChevronRight } else { IconName::ChevronDown }, T::ICON_SM * k))
                .tooltip(w::tooltip(if b.folded { "Unfold" } else { "Fold" }))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, _, cx| super::toggle_fold(&kf, &bf, cx))
        });
        let lh = fs * T::LH_CHAT;
        let mut body = div().flex().w_full().gap(T::SP_6).py(px(1.));
        match b.kind.as_str() {
            "todo" => {
                let (kc, bc, on) = (key.clone(), b.id.clone(), b.checked);
                // The to-do's box: the shared check (its own press, not the block's).
                body = body.child(div().flex_none().pl(T::SP_2).pr(T::SP_4).pt((lh - T::CHECK_BOX) / 2.)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(w::check(sid(format!("nt-chk-{key}-{}", b.id)), "", on, move |_, _, cx| super::edit_block(&kc, &bc, cx, |b| b.checked = !on))));
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
                self.text_cell(&key, &b.id, -1, -1, &b.text, doc::placeholder(&b.kind), fs, weight, ink, b.kind == "todo" && b.checked, false, false, None).into_any_element()
            }
        };
        let _ = cx;
        body = body.child(div().flex_1().min_w_0().child(content));
        // A nested paragraph or list item: under its parent's words.
        if doc::nesty(&b.kind) && b.level > 0 { body = body.pl((T::SP_20 + T::SP_2 + T::SP_6) * b.level as f32); }
        let (kbx, bbx) = (key.clone(), b.id.clone());
        div().id(sid(format!("nt-b-{key}-{ix}"))).group(group).relative().w_full().pt(top)
            .on_mouse_down(MouseButton::Left, move |e, _, cx| super::blk_press(&kp, &bp, e.modifiers.shift, cx))
            .child(canvas(move |bd, _, cx| super::store(cx).update(cx, |s, _| if let Some(i) = super::idx(s, &kbx) {
                let bx = &mut s.places[i].boxes;
                bx.retain(|x| x.0 != bbx);
                bx.push((bbx.clone(), f32::from(bd.origin.y), f32::from(bd.size.height)));
            }), |_, _, _, _| {}).absolute().top_0().left_0().size_full())
            .when(r.sel, |d| d.child(div().absolute().left(margin - T::SP_6).right_0().top_0().bottom_0().bg(T::accent_dim())
                .when(r.sel_top, |d| d.rounded_t(T::R_CONTROL)).when(r.sel_bottom, |d| d.rounded_b(T::R_CONTROL))))
            .child(margin_area)
            .child(tools)
            .children(fold)
            .child(div().pl(margin).child(body))
    }

    /// A block's (or cell's) text: the text box while it has the caret, the
    /// formatted text otherwise (a click puts the caret at its end; with
    /// Ctrl, a click on a link opens it, the web's).
    #[allow(clippy::too_many_arguments)]
    fn text_cell(&mut self, key: &str, id: &str, r: i32, c: i32, src: &str, ph: &str, fs: Pixels, weight: FontWeight, ink: Hsla, done: bool, mono: bool, nowrap: bool, lang: Option<&str>) -> AnyElement {
        let here = self.editing && self.edit.as_ref() == Some(&(id.to_string(), r, c));
        if here {
            let mut d = div().relative().w_full();
            if src.is_empty() && !ph.is_empty() {
                d = d.child(div().absolute().left_0().top_0().text_size(fs).line_height(relative(T::LH_CHAT)).font_weight(weight).text_color(T::text_muted()).child(ph.to_string()));
            }
            // The box at its smallest size pads its words by 4 px each side and
            // none above or below: pulled out by as much, the words stay
            // exactly where they are drawn at rest.
            let t = w::textarea(&self.edit_in).appearance(false).xsmall().w_full().text_size(fs).font_weight(weight).text_color(ink).line_height(relative(T::LH_CHAT));
            // Something dragged over the box: its caret follows the pointer.
            let (ein, k, i, src_s) = (self.edit_in.clone(), key.to_string(), id.to_string(), src.to_string());
            let d = d.on_drag_move::<w::RefDrag>(move |e, _, cx| {
                if e.drag(cx).kind == "note" && super::drag_key(cx).as_deref() == Some(k.as_str()) { return; }
                let p = e.event.position;
                if !e.bounds.contains(&p) { return super::drop_over(&k, &i, r, c, None, cx); }
                let Some(bx) = caret_near(&ein.read(cx), p) else { return };
                let at = if mono { bx } else { doc::box_to_src(&src_s, bx) };
                ein.update(cx, |s, cx| s.set_selected_range(bx..bx, cx));
                super::drop_over(&k, &i, r, c, Some((at, None)), cx);
            });
            return d.child(div().mx(-T::SP_4).child(if mono { t.font_family(T::MONO) } else { t })).into_any_element();
        }
        // The parse, once per source.
        let ck = format!("{id}-{r}-{c}");
        // (A code block's key carries its language: a new one colours it again.)
        let ck = match lang { Some(l) => format!("{ck}-{l}"), None => ck };
        let rich = match self.rich.get(&ck) {
            Some((s, d, x)) if s == src && *d == done => x.clone(),
            _ => {
                let x = Rc::new(match lang { Some(l) => parse_code(src, l), None => parse_rich(src, done) });
                self.rich.insert(ck, (src.to_string(), done, x.clone()));
                x
            }
        };
        // A drag-select under way in these words: its range on the selection's colour.
        let held = self.tsel.as_ref().filter(|t| t.key == key && t.id == id && t.r == r && t.c == c && t.a != t.b).map(|t| t.a.min(t.b)..t.a.max(t.b));
        let styled = match held {
            Some(sel) => StyledText::new(rich.text.clone()).with_highlights(with_sel(&rich.hl, sel, rich.text.len())),
            None => StyledText::new(rich.text.clone()).with_highlights(rich.hl.clone()),
        };
        let layout = styled.layout().clone();
        let (k, i) = (key.to_string(), id.to_string());
        let n = src.len();
        let src_s = src.to_string();
        let is_code = lang.is_some();
        // A press here and a drag across the words selects them (the box
        // opens on the release, with that selection).
        let (lay_d, lay_m, k_d, k_m, i_d, i_m, tl) = (layout.clone(), layout.clone(), k.clone(), k.clone(), i.clone(), i.clone(), rich.text.len());
        let hit_at = move |l: &TextLayout, p: Point<Pixels>| match l.index_for_position(p) { Ok(ix) | Err(ix) => ix.min(tl) };
        let src_at = { let (rich, src_s) = (rich.clone(), src_s.clone()); move |ix: usize| -> usize {
            if src_s.is_empty() { return n; }
            if is_code { return ix.min(src_s.len()); }
            let ix = ix.min(rich.text.len());
            let k = rich.text.get(..ix).map(|t| t.chars().count()).unwrap_or(0);
            doc::plain_to_src_at(&doc::src_to_runs_map(&src_s).1, k)
        } };
        // Something dragged over the words: the caret shows where it would go.
        let (lay_g, k_g, i_g, src_g) = (layout.clone(), k.clone(), i.clone(), src_at.clone());
        let caret_ix = self.dspot.as_ref().filter(|d| d.key == key && d.id == id && d.r == r && d.c == c).and_then(|d| d.ix);
        let lay_c = layout.clone();
        // (T13: not wrapped, the words are as wide as their longest line, at
        // least the box's: its sideways scroll has that to go through.)
        div().id(sid(format!("nt-t-{key}-{id}-{r}-{c}"))).when(!nowrap, |d| d.w_full()).when(nowrap, |d| d.flex_none().min_w_full()).cursor_text().font_weight(weight).text_color(ink)
            .on_drag_move::<w::RefDrag>(move |e, _, cx| {
                if e.drag(cx).kind == "note" && super::drag_key(cx).as_deref() == Some(k_g.as_str()) { return; }
                let p = e.event.position;
                if !e.bounds.contains(&p) { return super::drop_over(&k_g, &i_g, r, c, None, cx); }
                let ix = hit_at(&lay_g, p);
                super::drop_over(&k_g, &i_g, r, c, Some((src_g(ix), Some(ix))), cx);
            })
            .when_some(caret_ix, |d, ix| d.child(canvas(|_, _, _| {}, move |_, _, window, _| {
                if let Some(p) = lay_c.position_for_index(ix) {
                    window.paint_quad(fill(Bounds { origin: p, size: size(px(1.5), lay_c.line_height()) }, T::text()));
                }
            }).absolute().size_full()))
            .on_mouse_down(MouseButton::Left, move |e, window, cx| {
                if e.modifiers.shift || e.click_count > 1 { return; }
                let _ = window;
                let at = hit_at(&lay_d, e.position);
                super::tsel_start(super::TSel { key: k_d.clone(), id: i_d.clone(), r, c, a: at, b: at }, cx);
            })
            .on_mouse_move(move |e: &MouseMoveEvent, _, cx| {
                if e.pressed_button != Some(MouseButton::Left) || cx.has_active_drag() { return; }
                let at = hit_at(&lay_m, e.position);
                super::tsel_move(&k_m, &i_m, r, c, at, cx);
            })
            .on_click(move |e, window, cx| {
                let m = window.modifiers();
                let held = super::tsel_end(&k, cx).filter(|t| t.id == i && t.r == r && t.c == c && t.a != t.b);
                if let Some(t) = held { super::request_focus_sel(&k, &i, r, c, src_at(t.a), src_at(t.b), cx); return; }
                // Shift with a press here selected blocks (blk_press): no caret.
                if m.shift && super::has_sel(&k, cx) { return; }
                let hit = if e.position().x > px(0.) { layout.index_for_position(e.position()) } else { Err(rich.text.len()) };
                if (m.control || m.platform) && !rich.links.is_empty() {
                    if let Ok(ix) = hit {
                        if let Some(l) = rich.links.iter().find(|l| l.0.contains(&ix)) { super::open_link(&l.1, cx); }
                    }
                }
                // The caret where the click was (the web's caret from the
                // point), as a source offset; a click past the words: its end.
                let at = match hit {
                    // Code is its own source: the offset as it is.
                    Ok(ix) | Err(ix) if !src_s.is_empty() && is_code => ix.min(src_s.len()),
                    Ok(ix) | Err(ix) if !src_s.is_empty() => {
                        let ix = ix.min(rich.text.len());
                        let k = rich.text.get(..ix).map(|t| t.chars().count()).unwrap_or(0);
                        doc::plain_to_src_at(&doc::src_to_runs_map(&src_s).1, k)
                    }
                    _ => n,
                };
                super::request_focus(&k, &i, r, c, at, at, cx);
            })
            .child(div().when(!nowrap, |d| d.w_full()).text_size(fs).line_height(relative(T::LH_CHAT)).when(mono, |d| d.font_family(T::MONO)).when(nowrap, |d| d.whitespace_nowrap()).child(styled))
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
        let hl_lang = code_lang(b);
        // (Wrapped, it has no sideways scroll: none is kept for it.)
        if b.wrap { self.code_scroll.remove(&b.id); }
        let scroll = if b.wrap { ScrollHandle::new() } else { self.code_scroll.entry(b.id.clone()).or_insert_with(ScrollHandle::new).clone() };
        let code = self.text_cell(key, &b.id, -1, -1, &b.text, "Code", T::TEXT_12 * k, T::W_REGULAR, T::text(), false, true, !b.wrap, Some(&hl_lang));
        div().relative().w_full().pt(T::SP_24 + T::SP_4).pb(T::SP_4)
            .child(div().id(sid(format!("nt-code-{key}-{}", b.id))).w_full().p(T::SP_12).rounded(T::R_FIELD).bg(T::bg_secondary()).border_1()
                .border_color(if editing { T::accent_edge() } else { T::border_subtle() })
                // (T13: up and down go on to the page, as a browser's pre: no turning them sideways.)
                .when(!b.wrap && !editing, |d| d.flex().overflow_x_scroll().restrict_scroll_to_axis().track_scroll(&scroll))
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
                let t = self.text_cell(&key, &b.id, ri as i32, ci as i32, cell, "", T::TEXT_15 * k, if head { T::W_SEMIBOLD } else { T::W_REGULAR }, T::text(), false, false, false, None);
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
fn grip(key: &str, bid: &str, tip: &str, k: f32, click: bool, cx: &mut App) -> Stateful<Div> {
    let (k1, b1, k2, b2) = (key.to_string(), bid.to_string(), key.to_string(), bid.to_string());
    let g = w::square_button(sid(format!("nt-grip-{key}-{bid}")), IconName::GripVertical, tip.to_string(), w::SquareLook::Bare, T::SP_20, T::ICON_MD * k)
        .cursor_grab();
    // The drag carries the blocks' Markdown (the shared drag layer).
    let g = match super::drag_payload(key, bid, cx) {
        Some(p) => g.on_drag(p, move |d, _, _, cx| {
            super::drag_start(&k1, &b1, cx);
            let label = d.label.to_string();
            cx.new(|_| DragGhost(label))
        }),
        None => g,
    };
    if click { g.on_click(move |e, window, cx| { let p = e.position(); let p = if p.x > px(0.) { p } else { window.mouse_position() }; super::block_menu(&k2, &b2, f32::from(p.x), f32::from(p.y) + f32::from(T::SP_12), cx); }) } else { g }
}


// ─── What floats over the page: the menus, the format bar ───

impl PlaceView {
    fn floats(&self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (mine, menu, fmt) = { let s = self.store.read(cx); (s.float.as_ref().is_some_and(|f| f.key() == self.key), s.menu.clone(), s.fmt.clone()) };
        if !mine { return vec![]; }
        let mut out = vec![];
        let win = window.viewport_size();
        if menu.open {
            let x = menu.x.max(f32::from(T::SP_8)).min(f32::from(win.width - T::NOTES_MENU_W - T::SP_8));
            // The active item in view (the language menu opens at the current one).
            if menu.act >= 0 && self.menu_shown.get() != (menu.gen_n, menu.act) {
                self.menu_shown.set((menu.gen_n, menu.act));
                let before = menu.query_on as usize + (!menu.label.is_empty()) as usize;
                let seps = menu.items.iter().take(menu.act as usize + 1).filter(|it| it.sep).count();
                self.menu_scroll.scroll_to_item(before + seps + menu.act as usize);
            }
            let mut panel = w::menu_panel("nt-menu").w(T::NOTES_MENU_W).max_h(T::NOTES_MENU_MAX_H).overflow_y_scroll().track_scroll(&self.menu_scroll).p(T::SP_4).gap(px(0.))
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
            // Under its line; with no room below, above the line (the web's
            // place(): top + 330 past the window: the line's top - 330).
            let room = f32::from(T::NOTES_MENU_MAX_H) + 10.;
            let line = f32::from(T::TEXT_15) * T::LH_CHAT;
            let y = if menu.y + room > f32::from(win.height) { (menu.y - line - 6. - room).max(8.) } else { menu.y };
            out.push(deferred(anchored().position(point(px(x), px(y))).snap_to_window_with_margin(T::SP_8).child(panel)).with_priority(2).into_any_element());
        }
        if fmt.open {
            // (The shared menu panel, as a row.)
            let (size, me) = (self.fmt_size.clone(), cx.entity().downgrade());
            let measure = canvas(move |b, _, cx| {
                let now = (f32::from(b.size.width), f32::from(b.size.height));
                if size.get() != now { size.set(now); if let Some(me) = me.upgrade() { cx.defer(move |cx| me.update(cx, |_, cx| cx.notify())); } }
            }, |_, _, _, _| {}).absolute().top_0().left_0().size_full();
            let mut bar = w::menu_panel("nt-fmt").relative().flex_row().items_center().gap(T::SP_2).p(T::SP_4).child(measure);
            if !fmt.link {
                bar = bar.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .child(grip("__fmt", "", "Drag the selected text onto a chat or terminal", 1., false, cx));
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
            // Centred over the selection, 8 px above it (below it when there
            // is no room above), kept 8 px inside the window.
            let (bw, bh) = self.fmt_size.get();
            let line = f32::from(T::TEXT_15) * T::LH_CHAT;
            let above = fmt.y - bh - 8.;
            let y = if above < 8. { fmt.y + line + 8. } else { above };
            let x = (fmt.x - bw / 2.).min(f32::from(win.width) - bw - 8.).max(8.);
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

/// The words' highlights with a selection's range laid over them (split
/// where they meet: gpui wants them sorted and apart).
fn with_sel(hl: &[(std::ops::Range<usize>, HighlightStyle)], sel: std::ops::Range<usize>, len: usize) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let mut cuts: Vec<usize> = vec![0, len, sel.start.min(len), sel.end.min(len)];
    for (r, _) in hl { cuts.push(r.start); cuts.push(r.end); }
    cuts.sort_unstable(); cuts.dedup();
    let mut out = vec![];
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a >= b { continue; }
        let base = hl.iter().find(|(r, _)| r.start <= a && b <= r.end).map(|(_, h)| *h);
        let on = sel.start <= a && b <= sel.end;
        match (base, on) {
            (Some(h), false) => out.push((a..b, h)),
            (h, true) => out.push((a..b, HighlightStyle { background_color: Some(T::selection()), ..h.unwrap_or_default() })),
            (None, false) => {}
        }
    }
    out
}

/// The text box's offset nearest a point (its caret's spots, from the box's
/// last layout: on the pointer's line, the closest across it).
fn caret_near(st: &TextareaState, p: Point<Pixels>) -> Option<usize> {
    let v = st.value().to_string();
    let mut best: Option<(f32, f32, usize)> = None;
    for (i, _) in v.char_indices().map(|(i, ch)| (i, ch)).chain(std::iter::once((v.len(), ' '))) {
        let Some(b) = st.range_to_bounds(&(i..i)) else { continue };
        let (top, bot) = (f32::from(b.origin.y), f32::from(b.origin.y + b.size.height));
        let dy = if f32::from(p.y) < top { top - f32::from(p.y) } else if f32::from(p.y) > bot { f32::from(p.y) - bot } else { 0. };
        let dx = (f32::from(b.origin.x) - f32::from(p.x)).abs();
        if best.is_none_or(|(by, bx, _)| dy < by || (dy == by && dx < bx)) { best = Some((dy, dx, i)); }
    }
    best.map(|b| b.2)
}
