//! Cru's screen (app/vendor/crundi-voice.js open/render; the Slint client's
//! ui/desktop/cru.slint): a dropdown under the top bar, at the right. The
//! top row (Cru's mark, the usage legend, the state pill, the ⋯ menu, ×),
//! the field (the head in its state's colour), the words (what was heard,
//! the reply, the detail, a question's Yes/No, a notice), the chat view,
//! and the bottom row (the mic, the status word, keyboard mode, the hint).
//!
//! The live screen (cru_screen.rs): in the voice view its panel takes the
//! field's place; in the chat view it sits above the messages; `full`, it is
//! Cru's whole view over the project area (on a window at least
//! CRU_FULL_MIN_W wide), with Normal view and close. Kid mode (cru_kid.rs):
//! the kid screen over the whole window, the sky-to-sun background, big
//! type, the big mic and mute, replies in speech bubbles, the grown-ups'
//! lock and its PIN pad, the break scene.
//!
//! Drawn over the window (the shell's overlay layer), from cru.rs.

use crate::{cru, cru_blocks, cru_frame::CruFrame, cru_kid, cru_screen};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};

/// The mic's colours (MIC_COLORS): listen sky, think violet, speak green,
/// waking indigo, asleep grey, offline amber, problem red.
fn tint(p: &str) -> Hsla {
    match p { "listen" => T::sky(), "think" => T::accent_hover(), "speak" => T::green(), "waking" => T::accent(), "offline" => T::amber(), "problem" => T::red(), _ => T::text_muted() }
}

pub struct CruScreen { input: Entity<w::InputState>, frame: Entity<CruFrame>, focus: FocusHandle, shown: bool, scroll: ScrollHandle, turns: usize, _sub: Subscription }

impl CruScreen {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| w::InputState::new(window, cx).placeholder("Type to Cru"));
        let frame = cx.new(CruFrame::new);
        let sub = cx.subscribe_in(&input, window, |_, st, ev: &w::InputEvent, window, cx| if let w::InputEvent::PressEnter { .. } = ev {
            let t = st.read(cx).value().to_string();
            st.update(cx, |s, cx| s.set_value("", window, cx));
            cru::ask(&t, true);
        });
        CruScreen { input, frame, focus: cx.focus_handle(), shown: false, scroll: ScrollHandle::new(), turns: 0, _sub: sub }
    }

    /// After each pump: opened, it takes the keys (the box in keyboard mode); the frame draws again when its page moved.
    pub fn pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (open, kb) = cru::with(|c| (c.open, c.keyboard));
        if open != self.shown {
            self.shown = open;
            if open { if kb && !cru_kid::on() { self.input.update(cx, |s, cx| s.focus(window, cx)); } else { window.focus(&self.focus, cx); } }
            cx.notify();
        }
        self.frame.update(cx, |f, cx| f.pump(cx));
    }

    /// The live screen's panel: its head (title; Full view, or Normal view; ×) and its body (blocks, then the frame).
    fn screen_panel(&self, full: bool, kid: bool) -> Option<Div> {
        let scr = cru_screen::screen()?;
        let title = scr.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
        let video = scr.get("video").is_some();
        let target = cru_screen::with(|s| !s.target.is_empty());
        let blocks = scr.get("blocks").and_then(|b| b.as_array()).cloned().unwrap_or_default();
        let head = div().flex().flex_none().items_center().gap(T::SP_6)
            .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(title))
            .when(full, |d| d.child(w::button("cru-normal", "Normal view").icon(IconName::Minimize2).on_click(|_, _, _| cru_screen::normal_view())))
            .when(!full && !video, |d| d.child(w::icon_button("cru-full", IconName::Maximize2, "Full view").on_click(|_, _, _| cru_screen::full_view())))
            .child(w::icon_button("cru-scr-x", IconName::X, "Close what Cru shows").on_click(|_, _, _| cru_screen::close_screen()));
        let grow = full || kid;
        let mut body = div().flex().flex_col().gap(T::SP_8).when(grow, |d| d.flex_1().min_h_0());
        if !blocks.is_empty() { body = body.child(div().id("cru-blocks").max_h(T::CRU_SCREEN_H).overflow_y_scroll().child(cru_blocks::blocks(&blocks, kid))); }
        if target { body = body.child(div().when(grow, |d| d.flex_1().min_h_0()).when(!grow, |d| d.h(T::CRU_SCREEN_H)).child(self.frame.clone())); }
        Some(div().flex().flex_col().gap(T::SP_6).when(grow, |d| d.flex_1().min_h_0()).when(!kid, |d| d.child(head)).child(body))
    }

    fn kid_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let (phase, reply, heard) = cru::with(|c| (c.phase.clone(), c.reply.clone(), if c.interim.is_empty() { c.heard.clone() } else { c.interim.clone() }));
        let k = cru_kid::with(|k| (k.paused, k.muted, k.pin_at.is_some(), k.pin.len(), k.pin_err.clone()));
        let (paused, muted, pin, pin_len, pin_err) = k;
        let screen = self.screen_panel(false, true);
        let has_screen = screen.is_some();
        let top = div().flex().items_center().px(T::SP_24).pt(T::SP_16)
            .child(div().text_color(T::kid_button()).text_size(T::KID_HEADING).child(IconName::AudioLines))
            .child(div().flex_1())
            .child(w::icon_button("kid-lock", IconName::Lock, "Grown-ups").on_click(|_, _, _| cru_kid::pin_open()));
        let mut words = div().flex().flex_col().items_center().gap(T::SP_8).px(T::SP_24).w_full();
        if !heard.is_empty() {
            words = words.child(div().flex().w_full().max_w(T::KID_BUBBLE_W).justify_end()
                .child(div().px(T::SP_20).py(T::SP_10).rounded(T::R_PILL).bg(T::kid_heard_bg()).text_color(T::kid_heard()).text_size(T::KID_HEARD).font_weight(T::W_BOLD).child(heard)));
        }
        if !reply.is_empty() || phase == "think" {
            words = words.child(div().flex().flex_col().max_w(T::KID_BUBBLE_W).px(T::SP_24).py(T::SP_16).rounded(T::KID_REPLY_R).bg(T::white())
                .text_color(T::kid_text()).text_size(T::KID_REPLY).font_weight(T::W_BOLD).shadow_md()
                .when(phase == "think" && !reply.is_empty(), |d| d.child(div().text_size(T::TEXT_16).text_color(T::kid_wait()).child("Wait for me!")))
                .child(if phase == "think" && reply.is_empty() { "…".to_string() } else { reply }));
        }
        let mic = div().id("kid-mic").size(T::KID_MIC).rounded_full().bg(tint(&phase)).flex().items_center().justify_center()
            .text_color(T::white()).text_size(T::KID_MIC_ICON).cursor_pointer().shadow_lg().when(muted, |d| d.opacity(T::INBOX_TINT))
            .child(if phase == "speak" { IconName::Square } else { IconName::Mic }).on_click(|_, _, _| cru::mic());
        let mute = div().id("kid-mute").size(T::KID_BTN).rounded_full().bg(if muted { T::red_dim() } else { T::white() }).flex().items_center().justify_center()
            .text_color(if muted { T::red() } else { T::kid_button() }).text_size(T::ICON_XL).cursor_pointer().shadow_md()
            .child(if muted { IconName::MicOff } else { IconName::Volume2 }).on_click(|_, _, _| cru_kid::mute());
        let bottom = div().relative().flex().items_center().justify_center().py(T::SP_20).child(mic)
            .child(div().absolute().right(T::SP_24).child(mute));
        let mut root = div().id("kid-screen").absolute().inset_0().flex().flex_col()
            .bg(linear_gradient(175., linear_color_stop(T::kid_sky(), 0.), linear_color_stop(T::kid_sun(), 1.)))
            .text_color(T::kid_text())
            .track_focus(&self.focus)
            .on_key_down(|e: &KeyDownEvent, _, _| if cru_kid::pin_shown() {
                match e.keystroke.key.as_str() { "enter" => cru_kid::pin_key("OK"), "backspace" => cru_kid::pin_key("Back"), "escape" => cru_kid::pin_close(), k => cru_kid::pin_key(k) }
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(top)
            .when_some(screen, |d, s| d.child(div().flex().flex_col().flex_1().min_h_0().px(T::SP_24).child(s)))
            .when(!has_screen, |d| d.child(div().flex_1()))
            .child(words)
            .child(bottom);
        if paused {
            root = root.child(div().absolute().inset_0().flex().flex_col().items_center().justify_center().gap(T::SP_12).bg(T::kid_break())
                .text_color(T::kid_break_text()).text_size(T::KID_TEXT).font_weight(T::W_BOLD)
                .child(div().text_size(T::KID_MIC_ICON).child(IconName::Sun)).child("Break time! See you soon."));
        }
        if pin {
            let mut keys = div().flex().flex_wrap().justify_center().gap(T::SP_8);
            for key in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "Back", "0", "OK"] {
                keys = keys.child(div().id(("pin-key", key.as_bytes()[0] as usize + key.len())).size(T::KID_KEY).rounded_full().bg(if key == "OK" { T::kid_button() } else { T::bg_tertiary() })
                    .flex().items_center().justify_center().text_color(T::white()).text_size(T::TEXT_20).font_weight(T::W_BOLD).cursor_pointer()
                    .child(if key == "Back" { "←".to_string() } else { key.to_string() })
                    .on_click(move |_, _, _| cru_kid::pin_key(key)));
            }
            root = root.child(div().id("kid-pin").absolute().inset_0().flex().items_center().justify_center().bg(T::scrim())
                .on_click(|_, _, _| cru_kid::pin_close())
                .child(div().id("kid-pin-box").w(T::KID_PAD_W).p(T::SP_20).rounded(T::R_CARD).bg(T::bg_secondary()).text_color(T::text()).flex().flex_col().gap(T::SP_12).items_center()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(div().text_size(T::TEXT_14).text_center().child("Grown-ups: enter the PIN to leave kid mode"))
                    .child(div().h(T::CONTROL).text_size(T::TEXT_20).font_family(T::MONO).child("•".repeat(pin_len)))
                    .child(keys)
                    .when(!pin_err.is_empty(), |d| d.child(div().text_size(T::TEXT_13).text_color(T::red()).child(pin_err)))));
        }
        let _ = cx;
        root.into_any_element()
    }
}

impl Render for CruScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (open, phase, reply, detail, notice, question, keyboard, voice_off, chat_view, more, unavailable) = cru::with(|c| (c.open, c.phase.clone(), c.reply.clone(), c.detail.clone(), c.notice.clone(), c.question.clone(), c.keyboard, c.voice_off, c.chat_view, c.more, c.unavailable.clone()));
        if !open { return div().into_any_element(); }
        if cru_kid::on() { return self.kid_view(cx); }
        // Cru's full view: the project area (on a wide enough window).
        if cru_screen::full() && window.viewport_size().width >= T::CRU_FULL_MIN_W {
            if let Some(p) = self.screen_panel(true, false) {
                return div().absolute().top(T::TOPBAR_H).left_0().right_0().bottom_0().flex().flex_col().p(T::SP_16).bg(T::bg())
                    .track_focus(&self.focus)
                    .on_key_down(|e: &KeyDownEvent, _, _| if e.keystroke.key == "escape" { cru::close(); })
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(p).into_any_element();
            }
        }
        let tn = tint(&phase);
        let notice = if !unavailable.is_empty() { unavailable.clone() } else { notice };
        let status = cru::status_word(&phase, !unavailable.is_empty());
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
        let panel = self.screen_panel(false, false);
        let mut col = div().flex().flex_col().gap(T::SP_10).p(T::SP_12).child(top);
        // The voice view: the live screen takes the field's place.
        if panel.is_none() || chat_view {
            col = col.child(div().flex().justify_center()
                .child(div().relative().size(T::CRU_FIELD).rounded_full().border_2().border_color(tn).bg(tn.opacity(T::INBOX_TINT))
                    .flex().items_center().justify_center().text_color(T::accent()).text_size(T::CRU_HEAD).child(IconName::Bot)
                    .when(phase == "think", |d| d.child(div().absolute().bottom(T::SP_8).child(w::loading(""))))));
        }
        let panel_voice = !chat_view;
        let mut panel = panel;
        if panel_voice { if let Some(p) = panel.take() { col = col.child(p); } }
        col = col.child(div().text_center().text_size(T::TEXT_14).text_color(T::text_secondary()).child(cru::words()));
        if !reply.is_empty() && !chat_view { col = col.child(div().text_size(T::TEXT_15).text_color(T::text()).child(reply.clone())); }
        if !detail.is_empty() && detail != reply { col = col.child(div().text_size(T::TEXT_13).text_color(T::text_secondary()).child(detail)); }
        if !question.is_empty() {
            col = col.child(div().flex().justify_center().gap(T::SP_8)
                .child(w::primary("cru-yes", "Yes").on_click(|_, _, _| cru::ask("Yes", true)))
                .child(w::button("cru-no", "No").on_click(|_, _, _| cru::ask("No", true))));
        }
        if !notice.is_empty() { col = col.child(div().text_center().text_size(T::TEXT_12).text_color(T::text_muted()).child(notice)); }
        if chat_view {
            // The chat view: the live screen above the messages.
            if let Some(p) = panel.take() { col = col.child(p); }
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
