//! A chat pane (ARCHITECTURE.md, "The view tree"): ChatPane is layout only
//! (log over composer); the LogView and the Composer are its cached leaves.
//! The LogView observes the chat's store (its rows); the Composer observes
//! only the chat's meta (state, model, cost, error), so a streamed delta
//! never redraws the box.

use crate::store::{scale, ChatMeta, ChatStore, Row};
use crundi_core::tools;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::{input::InputEvent, text::TextView};
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
        // The composer is cached at the height it measured itself (it grows
        // with its text: a change of height notifies it, and so this).
        let h = self.composer.read(cx).height;
        div().flex().flex_col().size_full().min_h_0().bg(T::bg())
            .child(self.log.clone().cached(StyleRefinement::default().w_full().flex_1().min_h_0()))
            .child(self.composer.clone().cached(StyleRefinement::default().w_full().h(h).flex_none()))
    }
}

// ─── The log ───

/// The virtualised log: `gpui::list` over the store's rows; only the rows on
/// screen are laid out, and a row is measured again only when the store
/// splices it. Following the end (FollowMode::Tail) until the person scrolls up.
pub struct LogView {
    store: Entity<ChatStore>,
    list: ListState,
    /// Tool cards and thinking blocks open (by entry id).
    open: std::collections::HashSet<String>,
    /// The pane's text size step (its meta's), as last drawn.
    zoom: Option<i32>,
    _subs: Vec<Subscription>,
}

impl LogView {
    fn new(store: Entity<ChatStore>, cx: &mut Context<Self>) -> Self {
        let list = store.read(cx).list.clone();
        let subs = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            // The text size changes every row.
            cx.observe(&store.read(cx).meta.clone(), |this: &mut Self, m, cx| { let z = m.read(cx).0.zoom; if this.zoom != Some(z) { this.zoom = Some(z); cx.notify(); } }),
        ];
        LogView { store, list, open: Default::default(), _subs: subs, zoom: None }
            .init_zoom(cx)
    }

    fn init_zoom(mut self, cx: &App) -> Self { self.zoom = Some(self.store.read(cx).meta.read(cx).0.zoom); self }

    fn toggle(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.open.remove(id) { self.open.insert(id.to_string()); }
        if let Some(i) = self.store.read(cx).rows.iter().position(|r| r.id == id) { self.list.splice(i..i + 1, 1); }
        cx.notify();
    }

    fn item(&mut self, ix: usize, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let fs = scale(self.zoom.unwrap_or(0));
        let me = cx.entity().downgrade();
        let store = self.store.read(cx);
        let Some(r) = store.rows.get(ix) else { return div().into_any_element() };
        let open = self.open.contains(&r.id);
        let el = row(r, open, fs, &me);
        div().w_full().px(T::SP_16).pt(if ix == 0 { T::SP_12 } else { px(0.) }).pb(T::SP_10).child(el).into_any_element()
    }
}

/// A red box (an error, a failed turn's end).
fn error_box(text: SharedString, fs: f32) -> Div {
    div().w_full().px(T::SP_10).py(T::SP_6).rounded(T::R_CONTROL).border_1().border_color(T::red()).bg(T::red_dim())
        .text_color(T::red()).text_size(T::TEXT_12 * fs).child(text)
}

fn one_line(t: &str) -> String { t.split_whitespace().collect::<Vec<_>>().join(" ") }

/// One row as the PC client draws it (native-gpui's chat-log prose, tools,
/// thinking): the user's bubble, Claude's markdown, a tool card, thinking,
/// the end of a turn, an error, a notice.
fn row(r: &Row, open: bool, fs: f32, me: &WeakEntity<LogView>) -> AnyElement {
    let e = &r.shown;
    match e.kind {
        "user" => div().flex().justify_end()
            .child(div().max_w(relative(0.82)).px(T::SP_12).py(T::SP_8)
                .rounded_tl(T::R_BUBBLE).rounded_tr(T::R_BUBBLE).rounded_bl(T::R_BUBBLE).rounded_br(T::R_CHIP)
                .bg(T::accent_dim()).border_1().border_color(T::accent_edge())
                .text_size(T::TEXT_13 * fs).line_height(relative(T::LH_CHAT)).text_color(T::text())
                .child(r.text.clone()))
            .into_any_element(),
        "assistant" => match &r.md {
            Some(md) => div().w_full().text_size(T::TEXT_13 * fs).line_height(relative(T::LH_CHAT)).text_color(T::text())
                .child(TextView::new(md).selectable(true)).into_any_element(),
            None => div().into_any_element(),
        },
        "thinking" => {
            let size = T::TEXT_12 * fs;
            if e.text.is_empty() {
                return div().flex().items_center().gap(T::SP_6)
                    .child(w::dot(T::text_muted()).size(T::SP_6))
                    .child(div().italic().text_size(size).line_height(relative(T::LH_CHAT)).text_color(T::text_muted()).child(e.detail.clone()))
                    .into_any_element();
            }
            let live = e.status == "streaming";
            let id = e.id.clone();
            let label: AnyElement = if live { w::shimmer(SharedString::from(format!("thl-{}", e.id)), "Thinking").text_color(T::text_secondary()).into_any_element() }
                else { div().child("Thinking").into_any_element() };
            let head = div().id(SharedString::from(format!("thh-{}", e.id))).flex().items_center().gap(T::SP_6).cursor_pointer()
                .text_size(size).line_height(relative(T::LH_CHAT)).text_color(T::text_secondary())
                .child(w::icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }, T::ICON_SM * fs))
                .child(label)
                .on_click({ let me = me.clone(); move |_: &ClickEvent, _: &mut Window, cx: &mut App| { let _ = me.update(cx, |this, cx| this.toggle(&id, cx)); } });
            let body = div().id(SharedString::from(format!("thb-{}", e.id))).mt(T::SP_4)
                .max_h(T::COMPOSER_MAX + T::COMPOSER_MAX / 2.).overflow_y_scroll()
                .italic().text_size(size).line_height(relative(T::LH_CHAT)).text_color(T::text_muted())
                .child(r.text.clone());
            div().flex()
                .child(div().flex_none().w(T::SP_2).bg(T::border()))
                .child(div().flex().flex_col().flex_1().min_w_0().pl(T::SP_10 - T::SP_2).child(head).when(open, |d| d.child(body)))
                .into_any_element()
        }
        "tool" => {
            let id = e.id.clone();
            let mark = if e.status == "running" { w::spinner(T::ICON_SM).into_any_element() }
                else { div().flex_none().w(T::ICON_SM).flex().justify_center().child(w::dot(if e.error { T::red() } else { T::green() }).size(T::SP_6)).into_any_element() };
            let head = div().id(SharedString::from(format!("th-{}", e.id)))
                .flex().items_center().gap(T::SP_8).px(T::SP_10).py(T::SP_6).cursor_pointer()
                .child(mark)
                .child(div().flex_none().font_family(T::MONO).text_size(T::TEXT_12 * fs).font_weight(T::W_SEMIBOLD)
                    .text_color(if e.error { T::red() } else { T::accent_hover() }).child(e.title.clone()))
                .child(div().flex_1().min_w_0().text_size(T::TEXT_12 * fs).text_color(T::text_secondary()).truncate().child(one_line(&e.detail)))
                .child(div().flex_none().text_color(T::text_muted()).child(w::icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }, T::ICON_SM * fs)))
                .on_click({ let me = me.clone(); move |_: &ClickEvent, _: &mut Window, cx: &mut App| { let _ = me.update(cx, |this, cx| this.toggle(&id, cx)); } });
            let mut card = w::card().rounded(T::R_CONTROL).bg(T::bg_secondary()).border_color(T::border_subtle()).overflow_hidden().child(head);
            if open {
                let text = |t: &str, c: Hsla| div().font_family(T::MONO).text_size(T::TEXT_12 * fs).text_color(c).whitespace_normal().child(t.to_string());
                let mut body = div().id(SharedString::from(format!("tb-{}", e.id))).flex().flex_col().px(T::SP_10).py(T::SP_8).max_h(T::COMPOSER_MAX * 2.).overflow_y_scroll();
                body = match &e.body {
                    tools::Body::Bash { command, description } => body.child(text(command, T::text_secondary())).when(!description.is_empty(), |d| d.child(text(description, T::text_muted()).pt(T::SP_6))),
                    tools::Body::Json => body.child(text(&e.args, T::text_secondary())),
                    // (The diff, write and todo bodies are the chat-log area's: PLAN.md.)
                    _ => body.child(text(&e.args, T::text_secondary())),
                };
                if !e.text.is_empty() {
                    body = body.child(div().mt(T::SP_8).mb(T::SP_8).h(px(1.)).w_full().bg(T::border_subtle())).child(text(&e.text, if e.error { T::red() } else { T::text_muted() }));
                }
                card = card.child(div().h(px(1.)).w_full().bg(T::border_subtle())).child(body);
            }
            card.into_any_element()
        }
        "result" if e.error => error_box(if e.text.is_empty() { "Error".into() } else { r.text.clone() }, fs).into_any_element(),
        "result" => div().flex().flex_col().w_full()
            .child(div().w_full().h(px(1.)).bg(T::border_subtle()))
            .child(div().w_full().py(T::SP_4).text_center().text_size(T::TEXT_11 * fs).text_color(T::text_muted()).child(e.detail.clone()))
            .into_any_element(),
        "error" => error_box(r.text.clone(), fs).into_any_element(),
        "notice" => div().w_full().text_center().text_size(T::TEXT_12 * fs).italic().text_color(T::text_muted()).child(r.text.clone()).into_any_element(),
        // Permission and question cards are the chat-log area's (PLAN.md 2.13).
        _ => div().w_full().text_size(T::TEXT_12 * fs).text_color(T::text_secondary()).child(if e.title.is_empty() { r.text.clone() } else { e.title.clone().into() }).into_any_element(),
    }
}

impl Render for LogView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().id("log").size_full().min_h_0().bg(T::bg())
            .child(list(self.list.clone(), cx.processor(|this, ix: usize, window, cx| this.item(ix, window, cx))).size_full())
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
