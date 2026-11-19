//! A chat pane (ARCHITECTURE.md, "The view tree"): ChatPane is layout only
//! (log over composer); the LogView and the Composer are its cached leaves.
//! The LogView observes the chat's store (its rows); the Composer observes
//! only the chat's meta (state, model, cost, error), so a streamed delta
//! never redraws the box.

pub use crate::log::LogView;
use crate::store::{scale, ChatMeta, ChatStore};
use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::input::InputEvent;
use std::time::Duration;

pub struct ChatPane {
    pub store: Entity<ChatStore>,
    pub log: Entity<LogView>,
    pub composer: Entity<Composer>,
}

impl ChatPane {
    pub fn new(store: Entity<ChatStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let log = cx.new(|cx| LogView::new(store.clone(), cx));
        let composer = cx.new(|cx| Composer::new(store.clone(), window, cx));
        ChatPane { store, log, composer }
    }
}

impl Render for ChatPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("chat-pane");
        // The composer is cached at the height it measured itself (it grows
        // with its text: a change of height notifies it, and so this).
        let h = self.composer.read(cx).height;
        div().flex().flex_col().size_full().min_h_0().bg(T::bg())
            .child(self.log.clone().cached(StyleRefinement::default().w_full().flex_1().min_h_0()))
            .child(self.composer.clone().cached(StyleRefinement::default().w_full().h(h).flex_none()))
    }
}

// ─── The composer ───

/// The box with Send (Queue while Claude works) and Stop (one click arms
/// "Sure?" for 3 s, a second interrupts); the meta row: the state, the
/// model, the cost so far.
pub struct Composer {
    store: Entity<ChatStore>,
    meta: Meta2,
    input: Entity<w::TextareaState>,
    stop_armed: bool,
    /// Its content's height, as last laid out (its cached box's height).
    pub height: Pixels,
    _subs: Vec<Subscription>,
}

type Meta2 = crate::store::Meta;


impl Composer {
    fn new(store: Entity<ChatStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| w::TextareaState::new(window, cx).auto_grow(1, 9).placeholder("Message Claude…  (Enter to send, Shift+Enter for newline)"));
        let meta_e = store.read(cx).meta.clone();
        let subs = vec![
            cx.observe(&meta_e, |this: &mut Self, m: Entity<ChatMeta>, cx| { let n = m.read(cx).0.clone(); if this.meta != n { this.meta = n; cx.notify(); } }),
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { shift: false, secondary: false } = ev { this.send(window, cx); }
            }),
        ];
        let meta = meta_e.read(cx).0.clone();
        Composer { store, meta, input, stop_armed: false, height: px(96.), _subs: subs }
    }

    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |s, cx| s.set_value(text.to_string(), window, cx));
    }

    /// Send what is in the box (the Send button, Enter, the driver's "send").
    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        if text.trim().is_empty() { return; }
        self.input.update(cx, |s, cx| s.set_value(String::new(), window, cx));
        self.store.update(cx, |s, cx| s.send(text.trim_end().to_string(), cx));
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if self.stop_armed {
            self.stop_armed = false;
            self.store.update(cx, |s, cx| s.stop(cx));
        } else {
            self.stop_armed = true;
            cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                let _ = this.update(cx, |c, cx| if c.stop_armed { c.stop_armed = false; cx.notify(); });
            }).detach();
        }
        cx.notify();
    }
}

fn state_label(st: &str) -> (&'static str, Hsla) {
    match st {
        "working" => ("working…", T::accent_hover()),
        "needs-input" => ("needs your input", T::accent_hover()),
        "exited" => ("exited", T::text_muted()),
        _ => ("idle", T::text_muted()),
    }
}

impl Render for Composer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("composer");
        let m = &self.meta;
        let fs = scale(m.zoom);
        let st = crundi_core::head::composer_state(&m.state, m.sending);
        let busy = matches!(st.as_str(), "working" | "needs-input");
        let stop = (st == "working").then(|| w::stop_button("stop", self.stop_armed, fs).on_click(cx.listener(|this, _, _, cx| this.stop(cx))));
        let send = w::send_button("send", if busy { "Queue" } else { "Send" }, fs).on_click(cx.listener(|this, _, window, cx| this.send(window, cx)));
        let (label, label_c) = state_label(&st);
        let me = cx.entity();
        let measure = canvas(move |b, _, cx| {
            let h = b.size.height;
            // (After the frame: a notify while drawing would not draw again.)
            if (me.read(cx).height - h).abs() > px(0.5) { let me = me.clone(); cx.defer(move |cx| me.update(cx, |c, cx| { c.height = h; cx.notify(); })); }
        }, |_, _, _, _| {}).absolute().size_full();
        div().relative().flex().flex_col().w_full().gap(T::SP_6).px(T::SP_12).pt(T::SP_8).pb(T::SP_8).border_t_1().border_color(T::border()).bg(T::bg())
            .when(!m.error.is_empty(), |d| d.child(div().text_size(T::TEXT_12).text_color(T::red()).child(m.error.clone())))
            .child(div().flex().items_end().gap(T::SP_8).child(div().flex_1().min_w_0().child(w::message_box(&self.input, fs))).children(stop).child(send))
            .child(div().flex().items_center().min_w_0().overflow_hidden().gap(T::SP_8).text_size(T::TEXT_11 * fs).text_color(T::text_muted())
                .child(div().text_color(label_c).child(label))
                .child(div().flex_1())
                .when(!m.model.is_empty(), |d| d.child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(m.model.clone())))
                .when(m.cost > 0.0, |d| d.child(format!("${:.4}", m.cost))))
            .child(measure)
    }
}

