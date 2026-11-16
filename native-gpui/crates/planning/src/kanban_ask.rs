//! The ask dialog (the web's askText with its choice row; the Slint client's
//! src/planning/mod.rs `ask` and ui/planning/overlay.slint AskDialog): a
//! title, the label, a line (or a box for many lines), a row of pills to
//! choose from, Cancel and the action. Kanban's Add task and Edit task use it.
//! One at a time: a second replaces the first, which hears "cancelled".
//! The driver reads it as props `ask` and answers it with "ask-answer".

use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::rc::Rc;

/// The choice row: its label, the (key, text) options, the one chosen.
pub struct Choice { pub label: String, pub options: Vec<(String, String)>, pub value: String }

type AskFn = Box<dyn FnOnce(Option<(String, String)>, &mut Window, &mut App)>;

enum Field { Line(Entity<w::InputState>), Area(Entity<w::TextareaState>) }

struct Open {
    title: String,
    label: String,
    ok: String,
    choice: Option<Choice>,
    chosen: String,
    field: Field,
    then: Option<AskFn>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    _sub: Subscription,
}

#[derive(Default)]
struct AskG(Option<Open>);
impl Global for AskG {}

fn g(cx: &mut App) -> &mut AskG {
    if !cx.has_global::<AskG>() { cx.set_global(AskG::default()); }
    cx.global_mut::<AskG>()
}

/// Ask for text (and a choice): `then(Some((text, choice)))` on OK or Enter
/// (Ctrl+Enter in a box of many lines), `then(None)` on Cancel, ×, Esc or the scrim.
#[allow(clippy::too_many_arguments)]
pub fn ask(window: &mut Window, cx: &mut App, title: &str, label: &str, value: &str, multiline: bool, ok: &str, choice: Option<Choice>,
           then: impl FnOnce(Option<(String, String)>, &mut Window, &mut App) + 'static) {
    if let Some(mut old) = g(cx).0.take() { if let Some(f) = old.then.take() { f(None, window, cx); } }
    let v = value.to_string();
    let (field, sub) = if multiline {
        let e = cx.new(|cx| { let mut s = w::TextareaState::new(window, cx).auto_grow(6, 12); s.set_value(v, window, cx); s });
        e.update(cx, |s, cx| s.focus(window, cx));
        let sub = window.subscribe(&e, cx, |_, ev: &w::InputEvent, window, cx| {
            if let w::InputEvent::PressEnter { secondary: true, .. } = ev { answer(true, window, cx); }
        });
        (Field::Area(e), sub)
    } else {
        let e = cx.new(|cx| { let mut s = w::InputState::new(window, cx); s.set_value(v, window, cx); s });
        e.update(cx, |s, cx| { s.focus(window, cx); s.select_all(window, cx); });
        let sub = window.subscribe(&e, cx, |_, ev: &w::InputEvent, window, cx| {
            if let w::InputEvent::PressEnter { .. } = ev { answer(true, window, cx); }
        });
        (Field::Line(e), sub)
    };
    let chosen = choice.as_ref().map(|c| c.value.clone()).unwrap_or_default();
    let open = Open {
        title: title.into(), label: label.into(), ok: if ok.is_empty() { "OK".into() } else { ok.into() },
        choice, chosen, field, then: Some(Box::new(then)), focus: cx.focus_handle(), scroll: ScrollHandle::new(), _sub: sub,
    };
    g(cx).0 = Some(open);
    window.refresh();
}

fn value_of(o: &Open, cx: &App) -> String {
    match &o.field { Field::Line(e) => e.read(cx).value().to_string(), Field::Area(e) => e.read(cx).value().to_string() }
}

/// Close it: OK (with the text and the choice) or not.
pub fn answer(ok: bool, window: &mut Window, cx: &mut App) {
    let Some(mut o) = g(cx).0.take() else { return };
    let r = ok.then(|| (value_of(&o, cx), o.chosen.clone()));
    window.refresh();
    if let Some(f) = o.then.take() { f(r, window, cx); }
}

pub fn is_open(cx: &App) -> bool { cx.try_global::<AskG>().is_some_and(|a| a.0.is_some()) }

/// Sign-out: a dialog open hears "cancelled".
pub fn cancel(window: &mut Window, cx: &mut App) { answer(false, window, cx); }

/// The driver's "ask-answer <ok> [text] [choice]" (empty: as they are).
pub fn driver_answer(args: &[String], window: &mut Window, cx: &mut App) -> Value {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    if !is_open(cx) { return json!({ "ok": false, "error": "no ask dialog" }); }
    let (text, ch) = (a(1), a(2));
    if !text.is_empty() {
        let f = cx.global::<AskG>().0.as_ref().map(|o| match &o.field { Field::Line(e) => (Some(e.clone()), None), Field::Area(e) => (None, Some(e.clone())) });
        match f {
            Some((Some(e), _)) => e.update(cx, |s, cx| s.set_value(text, window, cx)),
            Some((_, Some(e))) => e.update(cx, |s, cx| s.set_value(text, window, cx)),
            _ => {}
        }
    }
    if !ch.is_empty() { if let Some(o) = g(cx).0.as_mut() { o.chosen = ch; } }
    answer(a(0) == "true", window, cx);
    json!({ "ok": true })
}

/// What the driver sees (the Slint client's names), or Null.
pub fn props(cx: &App) -> Value {
    let Some(o) = cx.try_global::<AskG>().and_then(|a| a.0.as_ref()) else { return Value::Null };
    json!({ "title": o.title, "label": o.label, "value": value_of(o, cx), "multiline": matches!(o.field, Field::Area(_)), "ok": o.ok,
        "choiceLabel": o.choice.as_ref().map(|c| c.label.clone()).unwrap_or_default(),
        "choices": o.choice.as_ref().map(|c| c.options.iter().map(|x| x.0.clone()).collect::<Vec<_>>()).unwrap_or_default(),
        "choice": o.chosen })
}

/// The dialog over the window, when open.
pub fn render(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let o = cx.try_global::<AskG>()?.0.as_ref()?;
    let mut body: Vec<AnyElement> = vec![
        w::modal_head("kanban-ask-x", o.title.clone(), false, |_, window, cx| answer(false, window, cx)).into_any_element(),
    ];
    let mut form = div().flex().flex_col().gap(T::SP_8).px(T::SP_8).pb(T::SP_4);
    if !o.label.is_empty() { form = form.child(w::field_label(o.label.clone())); }
    form = match &o.field {
        Field::Line(e) => form.child(w::input(e)),
        Field::Area(e) => form.child(w::textarea(e)),
    };
    if let Some(c) = o.choice.as_ref().filter(|c| !c.options.is_empty()) {
        let mut pills = div().flex().flex_wrap().gap(T::SP_6);
        for (i, (k, t)) in c.options.iter().enumerate() {
            let key = k.clone();
            pills = pills.child(w::toggle(("kanban-ask-pill", i), t.clone(), o.chosen == *k).on_click(move |_, window, cx| {
                if let Some(o) = g(cx).0.as_mut() { o.chosen = key.clone(); }
                window.refresh();
            }));
        }
        form = form.child(div().flex().flex_col().gap(T::SP_6).pt(T::SP_2)
            .when(!c.label.is_empty(), |d| d.child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(c.label.clone())))
            .child(pills));
    }
    body.push(form.into_any_element());
    body.push(div().flex().justify_end().gap(T::SP_8).p(T::SP_8)
        .child(w::button("kanban-ask-cancel", "Cancel").on_click(|_, window, cx| answer(false, window, cx)))
        .child(w::primary("kanban-ask-ok", o.ok.clone()).on_click(|_, window, cx| answer(true, window, cx)))
        .into_any_element());
    let m = w::Modal {
        id: "kanban-ask",
        width: T::DIALOG_W_LG,
        focus: o.focus.clone(),
        scroll: o.scroll.clone(),
        on_close: Rc::new(|window, cx| answer(false, window, cx)),
        on_key: Rc::new(|e: &KeyDownEvent, window, cx| if e.keystroke.key == "escape" { answer(false, window, cx); }),
    };
    Some(w::overlay_modal(m, window, body).into_any_element())
}
