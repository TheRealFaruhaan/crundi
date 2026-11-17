//! The Inbox page (src/webapp-html.js #inbox-panel, ibxRender; the Slint
//! client's ui/desktop/inbox.slint): a full page with no tab button of its
//! own (the sidebar's Inbox opens it). The head (title, the unread line,
//! Mark all read), the threads on the left, the open conversation on the
//! right: oldest first, no reply box (Open chat). Drawn from inbox.rs.

use crate::{ctx as kit, inbox};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};

/// A thread's glyph and colour (IBX_THREAD_IC).
fn thread_look(k: &str) -> (IconName, Hsla) {
    match k { "project" => (IconName::Folder, T::sky()), "cru" => (IconName::Mic, T::green()), "system" => (IconName::Info, T::amber()), _ => (IconName::MessageSquare, T::accent_hover()) }
}

/// A message kind's glyph and colour (IBX_KIND).
fn kind_look(k: &str) -> (IconName, Hsla) {
    let tint = match k { "alert" | "permission" => T::amber(), "question" => T::sky(), _ => T::text_secondary() };
    let icon = match k {
        "photo" => IconName::Image, "file" => IconName::File, "alert" => IconName::Bell, "question" => IconName::CircleQuestionMark,
        "permission" => IconName::ShieldAlert, "report" => IconName::Mic, "schedule" => IconName::Clock, "system" => IconName::Info, _ => IconName::MessageSquare,
    };
    (icon, tint)
}

/// The thread's round avatar with its glyph.
fn avatar(kind: &str) -> Div {
    let (icon, tint) = thread_look(kind);
    div().flex_none().size(T::CONTROL).rounded_full().bg(tint.opacity(T::INBOX_TINT)).flex().items_center().justify_center()
        .text_color(tint).text_size(T::ICON_MD).child(icon)
}

pub struct InboxPage { pinned: u64, scroll: ScrollHandle }

impl InboxPage {
    pub fn new(_cx: &mut Context<Self>) -> Self { InboxPage { pinned: 0, scroll: ScrollHandle::new() } }
}

impl Render for InboxPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let (threads, state, open, items, more, fresh, pics, pin) = inbox::with(|i| (i.threads.clone(), i.state.clone(), i.open.clone(), i.items.clone(), i.more, i.fresh.clone(), i.pics.clone(), i.pin_gen));
        if pin != self.pinned { self.pinned = pin; self.scroll.scroll_to_bottom(); }
        let now = inbox::now_ms();
        let head = w::toolbar().px(T::SP_16).h(T::CONTROL_LG + T::SP_12)
            .child(div().text_size(T::ICON_LG).text_color(T::text()).child(IconName::Mail))
            .child(div().text_size(T::TEXT_16).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Inbox"))
            .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child(inbox::sub()))
            .child(div().flex_1())
            .child(w::button("mark-all", "Mark all read").icon(IconName::Check).on_click(|_, _, _| inbox::mark_all()));
        // The threads.
        let mut list = div().id("ibx-list").flex().flex_col().flex_none().w(T::INBOX_LIST_W).h_full().border_r_1().border_color(T::border()).overflow_y_scroll();
        if state == "loading" && threads.is_empty() { list = list.child(w::loading("Loading…")); }
        if state == "error" { list = list.child(w::error_state("ibx-retry", "Could not load the inbox.", |_, _, _| inbox::load())); }
        if state == "ok" && threads.is_empty() {
            list = list.child(w::empty("Nothing here yet. Messages, pictures and files from your chats land here, with questions, alerts and reports."));
        }
        for (n, t) in threads.iter().enumerate() {
            let key = kit::s(t, "key");
            let last = t.get("last").cloned().unwrap_or_default();
            let u = kit::n(t, "unread") as i64;
            let last_line = Some(kit::s(&last, "title")).filter(|x| !x.is_empty()).unwrap_or_else(|| kit::s(&last, "text"));
            let sub = kit::s(t, "subtitle");
            let k2 = key.clone();
            list = list.child(div().id(("thread", n)).flex().gap(T::SP_10).px(T::SP_16).py(T::SP_12).cursor_pointer()
                .border_b_1().border_color(T::border_subtle())
                .when(key == open, |d| d.bg(T::accent_dim())).when(key != open, |d| d.hover(|d| d.bg(T::bg_hover())))
                .child(avatar(&kit::s(t, "kind")))
                .child(div().flex().flex_col().flex_1().min_w_0().gap(T::SP_2)
                    .child(div().truncate().text_size(T::TEXT_14).text_color(T::text()).font_weight(if u > 0 { T::W_BOLD } else { T::W_MEDIUM }).child(kit::s(t, "title")))
                    .when(!sub.is_empty(), |d| d.child(div().truncate().text_size(T::TEXT_11).text_color(T::text_muted()).child(sub)))
                    .child(div().truncate().text_size(T::TEXT_12).text_color(if u > 0 { T::text_secondary() } else { T::text_muted() }).child(last_line)))
                .child(div().flex().flex_col().items_end().gap(T::SP_4)
                    .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(inbox::when(now, kit::n(&last, "at"))))
                    .child(w::count_badge(if u > 0 { inbox::badge(u) } else { String::new() })))
                .on_click(move |_, _, _| inbox::open_thread(&k2)));
        }
        // The open conversation.
        let t = threads.iter().find(|t| kit::s(t, "key") == open).cloned();
        let detail: AnyElement = match t {
            None => w::empty(if threads.is_empty() { "Nothing here yet." } else { "Choose a conversation to read it." }).into_any_element(),
            Some(t) => {
                let kind = kit::s(&t, "kind");
                let sub = kit::s(&t, "subtitle");
                let chat = kind == "chat" && !kit::s(&t, "chatId").is_empty();
                let head = div().flex().flex_none().items_center().gap(T::SP_10).px(T::SP_16).py(T::SP_12).border_b_1().border_color(T::border())
                    .child(avatar(&kind))
                    .child(div().flex().flex_col().flex_1().min_w_0()
                        .child(div().truncate().font_weight(T::W_SEMIBOLD).text_size(T::TEXT_14).text_color(T::text()).child(kit::s(&t, "title")))
                        .when(!sub.is_empty(), |d| d.child(div().truncate().text_size(T::TEXT_12).text_color(T::text_muted()).child(sub))))
                    .when(chat, |d| d.child(w::button("open-chat", "Open chat").icon(IconName::MessageSquare).on_click(|_, _, _| inbox::open_chat())))
                    .child(w::button("mark-read", "Mark read").icon(IconName::Check).on_click(|_, _, _| inbox::mark_read()))
                    .child(w::danger("delete-thread", "Delete thread").icon(IconName::Trash).on_click(|_, _, _| inbox::delete_thread()));
                let mut msgs = div().flex().flex_col().gap(T::SP_10).p(T::SP_16);
                if more {
                    msgs = msgs.child(div().id("older").flex().justify_center().text_size(T::TEXT_12).text_color(T::accent_hover()).cursor_pointer().child("Show older").on_click(|_, _, _| inbox::older()));
                }
                // Oldest first.
                for (n, it) in items.iter().rev().enumerate() {
                    let id = kit::s(it, "id");
                    let kind = kit::s(it, "kind");
                    let (icon, tint) = kind_look(&kind);
                    let text = kit::s(it, "text");
                    let title = Some(kit::s(it, "title")).filter(|x| !x.is_empty()).unwrap_or_else(|| inbox::kind_label(&kind).to_string());
                    let mut card = w::card().max_w(T::INBOX_MSG_W).px(T::SP_16).pt(T::SP_10).pb(T::SP_12).gap(T::SP_6)
                        .when(fresh.contains(&id), |d| d.border_l_2().border_color(T::accent()))
                        .child(div().flex().items_center().gap(T::SP_8)
                            .child(div().text_size(T::ICON_SM).text_color(tint).child(icon))
                            .child(div().flex_1().min_w_0().truncate().font_weight(T::W_SEMIBOLD).text_size(T::TEXT_13).text_color(T::text()).child(title))
                            .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(inbox::when_full(kit::n(it, "at"))))
                            .child(w::icon_button(("msg-x", n), IconName::X, "Delete this message").on_click(move |_, _, _| inbox::delete_item(&id))));
                    if !text.is_empty() { card = card.child(div().text_size(T::TEXT_14).text_color(T::text()).child(text)); }
                    for (m_i, m) in it.get("media").and_then(|m| m.as_array()).cloned().unwrap_or_default().iter().enumerate() {
                        let url = kit::s(m, "url");
                        let pic = inbox::is_pic(&kit::s(m, "mime")) && !url.is_empty();
                        let u2 = url.clone();
                        let el = match pics.get(&url) {
                            Some(Some(img)) if pic => div().id(("pic", n * 100 + m_i)).cursor_pointer().child(gpui::img(ImageSource::Render(img.clone())).max_h(T::INBOX_PIC_H).object_fit(ObjectFit::Contain))
                                .on_click(move |_, _, _| inbox::media(&u2, true)).into_any_element(),
                            None if pic => w::loading("").into_any_element(),
                            _ => w::button(("file", n * 100 + m_i), format!("{}  {}", kit::s(m, "name"), inbox::size(kit::n(m, "size")))).icon(IconName::Download)
                                .on_click(move |_, _, _| if !u2.is_empty() { inbox::media(&u2, false) }).into_any_element(),
                        };
                        card = card.child(el);
                    }
                    msgs = msgs.child(card);
                }
                div().flex().flex_col().flex_1().min_w_0().h_full().child(head)
                    .child(div().id("ibx-conv").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.scroll).child(msgs)).into_any_element()
            }
        };
        div().flex().flex_col().size_full().bg(T::bg()).child(head)
            .child(div().flex().flex_1().min_h_0().child(list).child(div().flex().flex_1().min_w_0().h_full().child(detail)))
    }
}
