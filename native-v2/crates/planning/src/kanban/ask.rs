//! The card dialog (the web's askText with its choice row; native-gpui's
//! kanban_ask.rs): a title, the label, a line (or a box for many lines), a
//! row of pills to choose from, Cancel and the action. Kanban's Add task and
//! Edit task use it. One at a time: a second replaces the first, which hears
//! "cancelled". Drawn in the one modal layer (`w::modal`) as a view of its
//! own: choosing a pill redraws the dialog alone. The driver reads it as
//! `ask` and answers it with "ask-answer".

use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};

/// The choice row: its label, the (key, text) options, the one chosen.
pub struct Choice { pub label: String, pub options: Vec<(String, String)>, pub value: String }

type AskFn = Box<dyn FnOnce(Option<(String, String)>, &mut Window, &mut App)>;

enum Field { Line(Entity<w::InputState>), Area(Entity<w::TextareaState>) }

pub struct AskView {
    title: String,
    label: String,
    ok: String,
    choice: Option<Choice>,
    chosen: String,
    field: Field,
    _sub: Subscription,
}

/// The open dialog and what to do with its answer (a handle: read in handlers).
#[derive(Default)]
struct AskG(Option<(Entity<AskView>, Option<AskFn>)>);
impl Global for AskG {}

fn g(cx: &mut App) -> &mut AskG { cx.default_global::<AskG>() }

/// Ask for text (and a choice): `then(Some((text, choice)))` on OK or Enter
/// (Ctrl+Enter in a box of many lines), `then(None)` on Cancel or a press on
/// the scrim (the web's #input-modal: no ×, Esc does nothing).
#[allow(clippy::too_many_arguments)]
pub fn ask(window: &mut Window, cx: &mut App, title: &str, label: &str, value: &str, multiline: bool, ok: &str, choice: Option<Choice>,
           then: impl FnOnce(Option<(String, String)>, &mut Window, &mut App) + 'static) {
    if let Some((_, Some(f))) = g(cx).0.take() { f(None, window, cx); }
    let v = value.to_string();
    let (title, label, ok) = (title.to_string(), label.to_string(), if ok.is_empty() { "OK".to_string() } else { ok.to_string() });
    let view = cx.new(|cx| {
        let (field, sub) = if multiline {
            let e = cx.new(|cx| { let mut s = w::TextareaState::new(window, cx).auto_grow(6, 12); s.set_value(v, window, cx); s });
            e.update(cx, |s, cx| s.focus(window, cx));
            let sub = cx.subscribe_in(&e, window, |_, _, ev: &w::InputEvent, window, cx| {
                if let w::InputEvent::PressEnter { secondary: true, .. } = ev { answer(true, window, cx); }
            });
            (Field::Area(e), sub)
        } else {
            let e = cx.new(|cx| { let mut s = w::InputState::new(window, cx); s.set_value(v, window, cx); s });
            e.update(cx, |s, cx| { s.focus(window, cx); s.select_all(window, cx); });
            let sub = cx.subscribe_in(&e, window, |_, _, ev: &w::InputEvent, window, cx| {
                if let w::InputEvent::PressEnter { .. } = ev { answer(true, window, cx); }
            });
            (Field::Line(e), sub)
        };
        let chosen = choice.as_ref().map(|c| c.value.clone()).unwrap_or_default();
        AskView { title, label, ok, choice, chosen, field, _sub: sub }
    });
    g(cx).0 = Some((view.clone(), Some(Box::new(then))));
    // A press on the scrim cancels it.
    w::modal(window, cx, T::DIALOG_W_LG, view.into(), |window, cx| answer(false, window, cx));
}

fn value_of(o: &AskView, cx: &App) -> String {
    match &o.field { Field::Line(e) => e.read(cx).value().to_string(), Field::Area(e) => e.read(cx).value().to_string() }
}

/// Close it: OK (with the text and the choice) or not.
pub fn answer(ok: bool, window: &mut Window, cx: &mut App) {
    let Some((v, then)) = g(cx).0.take() else { return };
    let r = ok.then(|| { let o = v.read(cx); (value_of(o, cx), o.chosen.clone()) });
    // The modal layer closes (its on_close finds nothing open any more).
    if w::modal_open(cx) { w::close_dialog(window, cx); }
    if let Some(f) = then { f(r, window, cx); }
}

pub fn is_open(cx: &App) -> bool { cx.try_global::<AskG>().is_some_and(|a| a.0.is_some()) }

/// Sign-out: a dialog open hears "cancelled".
pub fn cancel(window: &mut Window, cx: &mut App) { answer(false, window, cx); }

/// The driver's "ask-answer <ok> [text] [choice]" (empty: as they are).
pub fn driver_answer(args: &[String], window: &mut Window, cx: &mut App) -> Value {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let Some(v) = cx.try_global::<AskG>().and_then(|g| g.0.as_ref().map(|o| o.0.clone())) else { return json!({ "ok": false, "error": "no ask dialog" }) };
    let (text, ch) = (a(1), a(2));
    if !text.is_empty() {
        match &v.read(cx).field {
            Field::Line(e) => { let e = e.clone(); e.update(cx, |s, cx| s.set_value(text, window, cx)) }
            Field::Area(e) => { let e = e.clone(); e.update(cx, |s, cx| s.set_value(text, window, cx)) }
        }
    }
    if !ch.is_empty() { v.update(cx, |o, _| o.chosen = ch); }
    answer(a(0) == "true", window, cx);
    json!({ "ok": true })
}

/// What the driver sees (native-gpui's names), or Null.
pub fn props(cx: &App) -> Value {
    let Some(v) = cx.try_global::<AskG>().and_then(|a| a.0.as_ref().map(|o| o.0.clone())) else { return Value::Null };
    let o = v.read(cx);
    json!({ "title": o.title, "label": o.label, "value": value_of(o, cx), "multiline": matches!(o.field, Field::Area(_)), "ok": o.ok,
        "choiceLabel": o.choice.as_ref().map(|c| c.label.clone()).unwrap_or_default(),
        "choices": o.choice.as_ref().map(|c| c.options.iter().map(|x| x.0.clone()).collect::<Vec<_>>()).unwrap_or_default(),
        "choice": o.chosen })
}

impl Render for AskView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The web's .im-box: the title, the label, the field, the choice, then
        // Cancel and the action side by side, sharing the row.
        let mut form = div().flex().flex_col().p(T::SP_16)
            .child(div().pb(T::SP_8).text_size(T::TEXT_16).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(self.title.clone()));
        if !self.label.is_empty() { form = form.child(div().pb(T::SP_10).text_size(T::TEXT_13).text_color(T::text_secondary()).child(self.label.clone())); }
        form = match &self.field {
            Field::Line(e) => form.child(w::input(e)),
            Field::Area(e) => form.child(w::textarea(e)),
        };
        if let Some(c) = self.choice.as_ref().filter(|c| !c.options.is_empty()) {
            let mut pills = div().flex().flex_wrap().gap(T::SP_6);
            for (i, (k, t)) in c.options.iter().enumerate() {
                let key = k.clone();
                // .im-choice button: a pill on the page's ground, outlined;
                // the chosen one tinted (aria-checked).
                let on = self.chosen == *k;
                pills = pills.child(w::toggled(("kanban-ask-pill", i), t.clone(), on).rounded(T::R_PILL)
                    .when(!on, |b| b.bg(T::bg()).text_color(T::text_secondary()))
                    .on_click(cx.listener(move |this, _, _, cx| { this.chosen = key.clone(); cx.notify(); })));
            }
            form = form.child(div().flex().flex_col().gap(T::SP_6).pt(T::SP_12)
                .when(!c.label.is_empty(), |d| d.child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(c.label.clone())))
                .child(pills));
        }
        form.child(div().flex().gap(T::SP_10).pt(T::SP_12)
            .child(div().flex_1().flex().child(w::button("kanban-ask-cancel", "Cancel").w_full().on_click(|_, window, cx| answer(false, window, cx))))
            .child(div().flex_1().flex().child(w::primary("kanban-ask-ok", self.ok.clone()).w_full().on_click(|_, window, cx| answer(true, window, cx)))))
    }
}
