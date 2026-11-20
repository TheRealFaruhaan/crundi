//! The Inbox page (src/webapp-html.js #inbox-panel, ibxRender; the Slint
//! client's ui/desktop/inbox.slint): a full page with no tab button of its
//! own (the sidebar's Inbox opens it). The head (title, the unread line,
//! Mark all read), the threads on the left, the open conversation on the
//! right: oldest first, no reply box (Open chat).
//!
//! **How it is built (ARCHITECTURE.md rules 1-5, 7).** `InboxPage` is the
//! tab's layout view (not cached, no text): it places three cached leaves,
//! the head, the threads and the conversation. Each leaf draws only from its
//! own copy, a typed struct taken from inbox.rs in `InboxPage::pump` (lib.rs
//! calls it when the logic is dirty; nothing is taken unless inbox.rs's
//! generation moved) and notified only when its copy changed. The two lists
//! are virtualised (`gpui::list`), spliced only for the rows that changed.
//! Every leaf paints in a layer (`w::layer`): they are replayed on every
//! frame the page shows.

use crate::{ctx as kit, inbox};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::Value;
use std::sync::Arc;

/// A thread's glyph and colour (IBX_THREAD_IC; .ibx-ic.t-*).
fn thread_look(k: &str) -> (IconName, Hsla) {
    match k {
        "chat" => (IconName::MessageSquare, T::accent_hover()),
        "project" => (IconName::Folder, T::sky()),
        "cru" => (IconName::Mic, T::green()),
        "system" => (IconName::Info, T::amber()),
        _ => (IconName::Mail, T::text_secondary()),
    }
}

/// A message kind's glyph and colour (IBX_KIND; .ibx-mh .k-*).
fn kind_look(k: &str) -> (IconName, Hsla) {
    let tint = match k { "alert" | "permission" => T::amber(), "question" => T::sky(), _ => T::text_muted() };
    let icon = match k {
        "photo" => IconName::Image, "file" => IconName::File, "alert" => IconName::Bell, "question" => IconName::CircleQuestionMark,
        "permission" => IconName::ShieldAlert, "report" => IconName::Mic, "schedule" => IconName::Clock, "system" => IconName::Info, _ => IconName::MessageSquare,
    };
    (icon, tint)
}

/// The thread's round picture (ibxThreadIcon): its glyph; a chat's or a
/// project's thread with the project's initials as a small badge.
fn avatar(kind: &str, badge: &str) -> Div {
    let (icon, tint) = thread_look(kind);
    div().relative().flex_none().size(T::CONTROL).rounded_full().bg(T::bg_tertiary()).flex().items_center().justify_center()
        .text_color(tint).text_size(T::ICON_LG).child(icon)
        .when(!badge.is_empty(), |d| d.child(div().absolute().right(-T::SP_2).bottom(-T::SP_2).size(T::SP_16).rounded(T::R_CHIP)
            .bg(T::accent()).border_1().border_color(T::bg_secondary()).flex().items_center().justify_center()
            .text_color(T::white()).text_size(T::TEXT_9).font_weight(T::W_BOLD).child(badge.to_string())))
}

/// The empty state (.ibx-empty): the envelope over a muted line.
fn empty(text: &'static str) -> Div {
    div().flex().flex_col().flex_1().items_center().justify_center().gap(T::SP_10).p(T::SP_24).text_center()
        .text_size(T::TEXT_13).text_color(T::text_muted())
        .child(div().opacity(0.6).text_size(T::CONTROL).child(IconName::Mail)).child(text)
}

/// The common head and tail of two lists: the range of `old` that changed,
/// and how many rows of `new` replace it (None: nothing changed).
pub fn changed<R: PartialEq>(old: &[R], new: &[R]) -> Option<(std::ops::Range<usize>, usize)> {
    let pre = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    if pre == old.len() && pre == new.len() { return None; }
    let max = old.len().min(new.len()) - pre;
    let suf = old.iter().rev().zip(new.iter().rev()).take(max).take_while(|(a, b)| a == b).count();
    Some((pre..old.len() - suf, new.len() - suf - pre))
}

/// A list's rows replaced by `new`: only the rows that changed are spliced.
fn splice<R: PartialEq>(list: &ListState, old: &mut Vec<R>, new: Vec<R>) -> bool {
    match changed(old, &new) {
        None => false,
        Some((range, n)) => { list.splice(range, n); *old = new; true }
    }
}

// ─── The copies ───

#[derive(Clone, PartialEq, Default)]
struct HeadCopy { sub: String }

#[derive(Clone, PartialEq)]
struct ThreadRow { key: String, kind: String, badge: String, title: String, sub: String, last: String, when: String, unread: i64, active: bool }

/// A picture's state: loading, read, or not read (then a file's button).
#[derive(Clone)]
enum Pic { Loading, Ready(Arc<RenderImage>), Failed }
impl PartialEq for Pic {
    fn eq(&self, o: &Self) -> bool {
        match (self, o) { (Pic::Loading, Pic::Loading) | (Pic::Failed, Pic::Failed) => true, (Pic::Ready(a), Pic::Ready(b)) => Arc::ptr_eq(a, b), _ => false }
    }
}

#[derive(Clone, PartialEq)]
struct Media { url: String, name: String, size: String, pic: Option<Pic> }

#[derive(Clone, PartialEq)]
struct Msg { id: String, kind: String, title: String, text: String, at: String, fresh: bool, media: Vec<Media> }

/// One row of the conversation: Show older, or a message (the last one has the bottom padding).
#[derive(Clone, PartialEq)]
enum Item { Older, Msg(Msg, bool) }

#[derive(Clone, PartialEq, Default)]
struct ConvHead { kind: String, title: String, sub: String, chat: bool }

#[derive(Clone, PartialEq, Default)]
struct ConvCopy { head: Option<ConvHead>, any: bool }

/// Everything taken from inbox.rs once per change (never in render).
fn take() -> (HeadCopy, Vec<ThreadRow>, bool, ConvCopy, Vec<Item>, u64) {
    let projects = kit::get().projects;
    let initials = |p: &str| -> String {
        if p.is_empty() { return String::new(); }
        let k = p.to_lowercase();
        let pr = projects.iter().find(|x| kit::s(x, "alias").to_lowercase() == k).or_else(|| projects.iter().find(|x| kit::s(x, "name").to_lowercase() == k));
        let name = pr.map(|x| Some(kit::s(x, "name")).filter(|n| !n.is_empty()).unwrap_or_else(|| kit::s(x, "alias"))).unwrap_or_else(|| p.to_string());
        crundi_core::head::project_initials(&name)
    };
    let now = inbox::now_ms();
    let sub = inbox::sub();
    inbox::with(|i| {
        let rows: Vec<ThreadRow> = i.threads.iter().map(|t| {
            let last = t.get("last").cloned().unwrap_or(Value::Null);
            let kind = kit::s(t, "kind");
            let badge = if kind == "chat" || kind == "project" { initials(&kit::s(t, "project")) } else { String::new() };
            ThreadRow {
                key: kit::s(t, "key"), badge, kind, title: kit::s(t, "title"), sub: kit::s(t, "subtitle"), last: kit::s(&last, "title"),
                when: inbox::when(now, kit::n(&last, "at")), unread: kit::n(t, "unread") as i64, active: kit::s(t, "key") == i.open,
            }
        }).collect();
        let t = i.threads.iter().find(|t| kit::s(t, "key") == i.open);
        let head = t.map(|t| ConvHead { kind: kit::s(t, "kind"), title: kit::s(t, "title"), sub: kit::s(t, "subtitle"), chat: kit::s(t, "kind") == "chat" && !kit::s(t, "chatId").is_empty() });
        let mut items = vec![];
        if head.is_some() {
            if i.more { items.push(Item::Older); }
            let n = i.items.len();
            // Oldest first.
            for (k, it) in i.items.iter().rev().enumerate() {
                let media = it.get("media").and_then(|m| m.as_array()).cloned().unwrap_or_default().iter().map(|m| {
                    let url = kit::s(m, "url");
                    let pic = (inbox::is_pic(&kit::s(m, "mime")) && !url.is_empty()).then(|| match i.pics.get(&url) {
                        None => Pic::Loading, Some(Some(img)) => Pic::Ready(img.clone()), Some(None) => Pic::Failed,
                    });
                    Media { name: kit::s(m, "name"), size: inbox::size(kit::n(m, "size")), url, pic }
                }).collect();
                let id = kit::s(it, "id");
                items.push(Item::Msg(Msg { fresh: i.fresh.contains(&id), id, kind: kit::s(it, "kind"), title: kit::s(it, "title"), text: kit::s(it, "text"), at: inbox::when_full(kit::n(it, "at")), media }, k + 1 == n));
            }
        }
        let loading = i.state == "loading" && i.threads.is_empty();
        (HeadCopy { sub }, rows, loading, ConvCopy { head, any: !i.threads.is_empty() }, items, i.pin_gen)
    })
}

// ─── The page ───

pub struct InboxPage {
    head: Entity<InboxHead>,
    threads: Entity<Threads>,
    conv: Entity<Conversation>,
    seen: Option<u64>,
    shown: bool,
    _nav: Subscription,
}

impl InboxPage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let head = cx.new(|_| InboxHead { copy: HeadCopy::default() });
        let threads = cx.new(|_| Threads { rows: vec![], loading: true, list: ListState::new(0, ListAlignment::Top, T::LOG_OVERDRAW) });
        let conv = cx.new(|_| {
            let list = ListState::new(0, ListAlignment::Top, T::LOG_OVERDRAW);
            list.set_follow_mode(FollowMode::Tail);
            Conversation { copy: ConvCopy::default(), items: vec![], pinned: 0, list }
        });
        // The page coming on show loads (openInbox): the shell's tab, heard
        // here (an observer runs no render).
        let nav = crundi_shell::stores(cx).nav.clone();
        let _nav = cx.observe(&nav, |this: &mut Self, nav, cx| {
            let on = nav.read(cx).tab == "inbox";
            if on && !this.shown { inbox::on_show(); }
            this.shown = on;
        });
        InboxPage { head, threads, conv, seen: None, shown: false, _nav }
    }

    /// The logic changed (lib.rs `pump`): copy what is drawn when inbox.rs
    /// moved; each leaf is notified only if its own copy changed.
    pub fn pump(&mut self, cx: &mut Context<Self>) {
        let g = inbox::generation();
        if self.seen == Some(g) { return; }
        self.seen = Some(g);
        let (head, rows, loading, conv, items, pin) = take();
        self.head.update(cx, |h, cx| if h.copy != head { h.copy = head; cx.notify(); });
        self.threads.update(cx, |t, cx| {
            let moved = splice(&t.list, &mut t.rows, rows);
            if moved || t.loading != loading { t.loading = loading; cx.notify(); }
        });
        self.conv.update(cx, |c, cx| {
            let mut moved = false;
            if c.copy != conv { c.copy = conv; moved = true; }
            if c.pinned != pin {
                // A thread (re)loaded: the newest message, followed again (ibx.pin).
                c.pinned = pin;
                let n = c.items.len();
                c.list.splice(0..n, items.len());
                c.items = items;
                c.list.set_follow_mode(FollowMode::Tail);
                moved = true;
            } else if splice(&c.list, &mut c.items, items) { moved = true; }
            if moved { cx.notify(); }
        });
    }
}

impl Render for InboxPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("inbox");
        div().flex().flex_col().size_full().bg(T::bg())
            .child(self.head.clone().cached(StyleRefinement::default().w_full().h(T::CONTROL_LG + T::SP_24).flex_none()))
            .child(div().flex().flex_1().min_h_0()
                .child(self.threads.clone().cached(StyleRefinement::default().w(T::INBOX_LIST_W).h_full().flex_none()))
                .child(self.conv.clone().cached(StyleRefinement::default().flex_1().min_w_0().h_full())))
    }
}

// ─── The head: Inbox, the unread line, Mark all read ───

pub struct InboxHead { copy: HeadCopy }

impl Render for InboxHead {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("inbox-head");
        w::layer(div().flex().items_center().size_full().gap(T::SP_10).px(T::SP_16).border_b_1().border_color(T::border_subtle())
            .child(div().flex().items_center().gap(T::SP_8).font_weight(T::W_SEMIBOLD).text_size(T::TEXT_16).text_color(T::text())
                .child(div().text_size(T::ICON_LG).child(IconName::Mail)).child("Inbox"))
            .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child(self.copy.sub.clone()))
            .child(div().flex_1())
            .child(w::button("mark-all", "Mark all read").icon(IconName::Check).on_click(|_, _, _| inbox::mark_all())))
    }
}

// ─── The threads ───

pub struct Threads { rows: Vec<ThreadRow>, loading: bool, list: ListState }

impl Threads {
    fn row(&self, ix: usize) -> AnyElement {
        let Some(t) = self.rows.get(ix) else { return div().into_any_element() };
        let key = t.key.clone();
        let u = t.unread;
        div().id(("ibx-thread", ix)).flex().items_start().gap(T::SP_10).px(T::SP_16).py(T::SP_12).cursor_pointer()
            .border_b_1().border_color(T::border_subtle())
            .when(t.active, |d| d.bg(T::accent_dim())).when(!t.active, |d| d.hover(|d| d.bg(T::bg_hover())))
            .child(avatar(&t.kind, &t.badge))
            .child(div().flex().flex_col().flex_1().min_w_0().gap(T::SP_2)
                .child(div().truncate().text_size(T::TEXT_14).text_color(T::text()).when(u > 0, |d| d.font_weight(T::W_SEMIBOLD)).child(t.title.clone()))
                .when(!t.sub.is_empty(), |d| d.child(div().truncate().text_size(T::TEXT_11).text_color(T::text_muted()).child(t.sub.clone())))
                .child(div().truncate().mt(T::SP_2).text_size(T::TEXT_12).text_color(if u > 0 { T::text_secondary() } else { T::text_muted() }).child(t.last.clone())))
            .child(div().flex().flex_none().flex_col().items_end().gap(T::SP_6).text_size(T::TEXT_11).text_color(T::text_muted())
                .child(t.when.clone())
                .when(u > 0, |d| d.child(w::count_badge(inbox::badge(u)))))
            .on_click(move |_, _, _| inbox::open_thread(&key))
            .into_any_element()
    }
}

impl Render for Threads {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("inbox-threads");
        let body = if self.rows.is_empty() {
            // (The web draws the empty list while the first load runs.)
            w::layer(empty("Nothing here yet. Messages, pictures and files from your chats land here, with questions, alerts and reports.").size_full()).into_any_element()
        } else {
            w::layer(list(self.list.clone(), cx.processor(|this: &mut Self, ix, _, _| this.row(ix))).size_full()).into_any_element()
        };
        div().size_full().border_r_1().border_color(T::border_subtle()).child(body)
    }
}

// ─── The open conversation ───

pub struct Conversation { copy: ConvCopy, items: Vec<Item>, pinned: u64, list: ListState }

impl Conversation {
    fn item(&self, ix: usize) -> AnyElement {
        match self.items.get(ix) {
            None => div().into_any_element(),
            Some(Item::Older) => div().pt(T::SP_16).px(T::SP_16).child(div().id("ibx-older").flex().justify_center().p(T::SP_10).text_size(T::TEXT_13)
                .text_color(T::accent_hover()).cursor_pointer().child("Show older").on_click(|_, _, _| inbox::older())).into_any_element(),
            Some(Item::Msg(m, last)) => div().w_full().px(T::SP_16).pt(if ix == 0 { T::SP_16 } else { T::SP_12 }).when(*last, |d| d.pb(T::SP_24))
                .child(message(ix, m)).into_any_element(),
        }
    }
}

/// A message's card (ibxMsgHtml): kind, title, time, ×; the text; its pictures and files.
fn message(ix: usize, m: &Msg) -> Div {
    let (icon, tint) = kind_look(&m.kind);
    let id = m.id.clone();
    let mut card = w::card().max_w(T::INBOX_MSG_W).px(T::SP_16).pt(T::SP_10).pb(T::SP_12).gap(T::SP_6).border_color(T::border_subtle())
        .when(m.fresh, |d| d.border_l_2().border_color(T::accent()))
        .child(div().flex().items_center().gap(T::SP_8)
            .child(div().id(("ibx-kind", ix)).text_size(T::ICON_MD).text_color(tint).child(icon).tooltip(w::tooltip(inbox::kind_label(&m.kind))))
            .child(div().flex_1().min_w_0().font_weight(T::W_SEMIBOLD).text_size(T::TEXT_14).text_color(T::text()).child(m.title.clone()))
            .child(div().flex_none().text_size(T::TEXT_11).text_color(T::text_muted()).child(m.at.clone()))
            .child(w::icon_button(("ibx-del", ix), IconName::X, "Delete this message").on_click(move |_, _, _| inbox::delete_item(&id))));
    if !m.text.is_empty() { card = card.child(div().text_size(T::TEXT_14).line_height(relative(1.55)).text_color(T::text_secondary()).child(m.text.clone())); }
    if !m.media.is_empty() {
        let mut media = div().flex().flex_col().items_start().gap(T::SP_8).mt(T::SP_4);
        for (k, x) in m.media.iter().enumerate() {
            let url = x.url.clone();
            let el = match &x.pic {
                Some(Pic::Ready(img)) => div().id(("ibx-pic", ix * 100 + k)).cursor_pointer().rounded(T::R_CONTROL).border_1().border_color(T::border_subtle()).overflow_hidden()
                    .child(gpui::img(ImageSource::Render(img.clone())).max_h(T::INBOX_PIC_H).object_fit(ObjectFit::Contain))
                    .on_click(move |_, _, _| inbox::media(&url, true)).into_any_element(),
                Some(Pic::Loading) => div().h(T::INBOX_PIC_H / 4.).w(T::INBOX_PIC_H).rounded(T::R_CONTROL).bg(T::bg_tertiary()).into_any_element(),
                // Not kept on the server: its name, muted.
                _ if url.is_empty() => div().flex().items_center().gap(T::SP_10).px(T::SP_12).py(T::SP_8).rounded(T::R_CONTROL).border_1().border_color(T::border())
                    .bg(T::bg_tertiary()).text_size(T::TEXT_13).text_color(T::text_muted())
                    .child(div().text_size(T::ICON_MD).child(IconName::File)).child(div().truncate().child(x.name.clone())).child(div().text_size(T::TEXT_12).child("not kept"))
                    .into_any_element(),
                _ => w::button(("ibx-file", ix * 100 + k), format!("{}  {}", x.name, x.size)).icon(IconName::Download)
                    .on_click(move |_, _, _| inbox::media(&url, false)).into_any_element(),
            };
            media = media.child(el);
        }
        card = card.child(media);
    }
    card
}

impl Render for Conversation {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("inbox-conv");
        let Some(t) = self.copy.head.clone() else {
            return w::layer(empty(if self.copy.any { "Choose a conversation to read it." } else { "Nothing here yet." }).size_full()).into_any_element();
        };
        let head = div().flex().flex_none().items_center().gap(T::SP_10).px(T::SP_16).py(T::SP_12).border_b_1().border_color(T::border_subtle())
            .child(avatar(&t.kind, ""))
            .child(div().flex().flex_col().min_w_0()
                .child(div().truncate().font_weight(T::W_SEMIBOLD).text_size(T::TEXT_15).text_color(T::text()).child(t.title.clone()))
                .when(!t.sub.is_empty(), |d| d.child(div().truncate().text_size(T::TEXT_12).text_color(T::text_muted()).child(t.sub.clone()))))
            .child(div().flex_1())
            .when(t.chat, |d| d.child(w::button("open-chat", "Open chat").icon(IconName::MessageSquare).on_click(|_, _, _| inbox::open_chat())))
            .child(w::button("mark-read", "Mark read").icon(IconName::Check).on_click(|_, _, _| inbox::mark_read()))
            .child(w::danger("delete-thread", "Delete thread").icon(IconName::Trash).on_click(|_, _, _| inbox::delete_thread()));
        div().flex().flex_col().size_full()
            .child(w::layer(head))
            .child(div().flex_1().min_h_0().child(w::layer(list(self.list.clone(), cx.processor(|this: &mut Self, ix, _, _| this.item(ix))).size_full())))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::changed;

    #[test]
    fn only_what_changed_is_spliced() {
        assert_eq!(changed(&[1, 2, 3], &[1, 2, 3]), None);
        assert_eq!(changed::<i32>(&[], &[]), None);
        assert_eq!(changed(&[1, 2, 3], &[1, 9, 3]), Some((1..2, 1)));
        assert_eq!(changed(&[1, 2, 3], &[1, 2, 3, 4]), Some((3..3, 1)));
        assert_eq!(changed(&[1, 2, 3], &[0, 1, 2, 3]), Some((0..0, 1)));
        assert_eq!(changed(&[1, 2, 3], &[1, 3]), Some((1..2, 0)));
        assert_eq!(changed(&[1, 1], &[1, 1, 1]), Some((2..2, 1)));
        assert_eq!(changed(&[1, 2], &[]), Some((0..2, 0)));
        assert_eq!(changed(&[], &[5]), Some((0..0, 1)));
    }
}
