//! Cru's screen (app/vendor/crundi-voice.js open/render; the Slint client's
//! ui/desktop/cru.slint): a dropdown under the top bar, at the right. The
//! top row (Cru's mark, the usage legend, the state pill, the ⋯ menu, ×),
//! the field (the head in its state's colour), the words (what was heard,
//! the reply, the detail, a question's Yes/No, a notice), the chat view,
//! and the bottom row (the mic, the status word, keyboard mode, the hint).
//! Drawn over the window (the shell's overlay layer), from cru.rs.

use crate::cru;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};

/// The mic's colours (MIC_COLORS): listen sky, think violet, speak green,
/// waking indigo, asleep grey, offline amber, problem red.
fn tint(p: &str) -> Hsla {
    match p { "listen" => T::sky(), "think" => T::accent_hover(), "speak" => T::green(), "waking" => T::accent(), "offline" => T::amber(), "problem" => T::red(), _ => T::text_muted() }
}

pub struct CruScreen { input: Entity<w::InputState>, focus: FocusHandle, shown: bool, scroll: ScrollHandle, turns: usize, _sub: Subscription }

impl CruScreen {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| w::InputState::new(window, cx).placeholder("Type to Cru"));
        let sub = cx.subscribe_in(&input, window, |_, st, ev: &w::InputEvent, window, cx| if let w::InputEvent::PressEnter { .. } = ev {
            let t = st.read(cx).value().to_string();
            st.update(cx, |s, cx| s.set_value("", window, cx));
            cru::ask(&t, true);
        });
        CruScreen { input, focus: cx.focus_handle(), shown: false, scroll: ScrollHandle::new(), turns: 0, _sub: sub }
    }

    /// After each pump: opened, it takes the keys (the box in keyboard mode).
    pub fn pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (open, kb) = cru::with(|c| (c.open, c.keyboard));
        if open != self.shown {
            self.shown = open;
            if open { if kb { self.input.update(cx, |s, cx| s.focus(window, cx)); } else { window.focus(&self.focus, cx); } }
            cx.notify();
        }
    }
}

impl Render for CruScreen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (open, phase, reply, detail, notice, question, keyboard, voice_off, chat_view, more, unavailable) = cru::with(|c| (c.open, c.phase.clone(), c.reply.clone(), c.detail.clone(), c.notice.clone(), c.question.clone(), c.keyboard, c.voice_off, c.chat_view, c.more, c.unavailable.clone()));
        if !open { return div().into_any_element(); }
        let tn = tint(&phase);
        let notice = if !unavailable.is_empty() { unavailable.clone() } else { notice };
        let status = cru::status_word(&phase, !unavailable.is_empty());
        let me = cx.entity().downgrade();
        let _ = me;
        let menu = vec![
            w::PickItem { id: "chat-view".into(), label: "Chat view".into(), kind: "item", on: chat_view, icon: Some(IconName::MessagesSquare) },
            w::PickItem { id: "voice-off".into(), label: "Voice off (read the replies)".into(), kind: "item", on: voice_off, icon: Some(IconName::VolumeX) },
            w::PickItem { id: "compact".into(), label: "Compact the conversation…".into(), kind: "item", on: false, icon: Some(IconName::Archive) },
        ];
        let top = div().flex().items_center().gap(T::SP_8)
            .child(div().text_color(T::accent()).text_size(T::TEXT_20).child(IconName::AudioLines))
            .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_11).text_color(T::text_muted()).child(cru::legend()))
            .child(div().px(T::SP_8).rounded(T::R_PILL).border_1().border_color(tn).bg(tn.opacity(T::INBOX_TINT)).text_color(tn).text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).child(cru::pill(&phase)))
            .child(w::pick_menu("cru-menu", IconName::Ellipsis, "More", "", move || menu.clone(), |id, _, _| cru::menu_pick(id)))
            .child(w::icon_button("cru-x", IconName::X, "Close (Esc)").on_click(|_, _, _| cru::close()));
        let field = div().flex().justify_center()
            .child(div().relative().size(T::CRU_FIELD).rounded_full().border_2().border_color(tn).bg(tn.opacity(T::INBOX_TINT))
                .flex().items_center().justify_center().text_color(T::accent()).text_size(T::CRU_HEAD).child(IconName::Bot)
                .when(phase == "think", |d| d.child(div().absolute().bottom(T::SP_8).child(w::loading("")))));
        let mut col = div().flex().flex_col().gap(T::SP_10).p(T::SP_12).child(top).child(field)
            .child(div().text_center().text_size(T::TEXT_14).text_color(T::text_secondary()).child(cru::words()));
        if !reply.is_empty() && !chat_view { col = col.child(div().text_size(T::TEXT_15).text_color(T::text()).child(reply.clone())); }
        if !detail.is_empty() && detail != reply { col = col.child(div().text_size(T::TEXT_13).text_color(T::text_secondary()).child(detail)); }
        if !question.is_empty() {
            col = col.child(div().flex().justify_center().gap(T::SP_8)
                .child(w::primary("cru-yes", "Yes").on_click(|_, _, _| cru::ask("Yes", true)))
                .child(w::button("cru-no", "No").on_click(|_, _, _| cru::ask("No", true))));
        }
        if !notice.is_empty() { col = col.child(div().text_center().text_size(T::TEXT_12).text_color(T::text_muted()).child(notice)); }
        if chat_view {
            let turns = cru::turns();
            if turns.len() != self.turns { self.turns = turns.len(); self.scroll.scroll_to_bottom(); }
            let mut list = div().flex().flex_col().gap(T::SP_6);
            if more { list = list.child(div().id("cru-older").flex().justify_center().text_size(T::TEXT_12).text_color(T::accent_hover()).cursor_pointer().child("Earlier messages").on_click(|_, _, _| cru::older())); }
            for (n, (mine, text)) in turns.into_iter().enumerate() {
                let t2 = text.clone();
                list = list.child(div().flex().when(mine, |d| d.justify_end())
                    .child(div().id(("turn", n)).max_w(T::CRU_TURN_W).px(T::SP_12).py(T::SP_6).rounded(T::R_BUBBLE)
                        .bg(if mine { T::accent_dim() } else { T::bg_tertiary() }).text_size(T::TEXT_13).text_color(T::text()).child(text)
                        .when(!mine, |d| d.cursor_pointer().on_click(move |e, _, _| if e.click_count() >= 2 { cru::say(&t2, false); }))));
            }
            col = col.child(div().id("cru-chat").h(T::CRU_CHAT_H).overflow_y_scroll().track_scroll(&self.scroll).child(list));
        }
        if keyboard { col = col.child(w::input(&self.input)); }
        col = col.child(div().flex().items_center().gap(T::SP_8)
            .child(w::icon_button("cru-attach", IconName::Paperclip, "Attach (not in this client yet)"))
            .child(div().flex_1())
            .child(w::icon_toggle("cru-mic", if phase == "speak" { IconName::Square } else { IconName::Mic }, if phase == "speak" { "Stop Cru speaking" } else { "Talk to Cru" }, phase == "listen").on_click(|_, _, _| cru::mic()))
            .child(div().flex_1().flex().justify_end().text_size(T::TEXT_12).text_color(T::text_muted()).child(status))
            .child(w::icon_toggle("cru-kb", IconName::Keyboard, if keyboard { "Back to talking" } else { "Keyboard mode: type to Cru" }, keyboard).on_click(|_, _, _| { let k = cru::with(|c| c.keyboard); cru::set_keyboard(!k); })))
            .child(div().text_center().text_size(T::TEXT_11).text_color(T::text_muted()).child("Click the mic to talk · Esc closes"));
        div().absolute().top(T::TOPBAR_H + T::SP_4).right(T::CRU_RIGHT).w(T::CRU_W).max_h(T::CRU_H)
            .track_focus(&self.focus)
            .on_key_down(|e: &KeyDownEvent, _, _| if e.keystroke.key == "escape" { cru::close(); })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .bg(T::bg_secondary()).rounded(T::R_CARD).border_1().border_color(T::border()).shadow_lg().overflow_hidden()
            .child(col)
            .into_any_element()
    }
}
