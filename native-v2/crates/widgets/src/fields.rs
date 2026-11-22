//! The form fields the web takes from the browser (F83): `<input
//! type="number">` (`number_state` / `number_input`), `type="date"`,
//! `type="time"` and `type="datetime-local"` (`date_state` / `date_input`,
//! `time_state` / `time_input`, `datetime_state`), the web's wrapping
//! option buttons (`.sm-seg` / `.sm-segb`: `option_buttons`) and a
//! read-only box whose text is selected whole on a click (the web's
//! `<input readonly onclick="this.select()">` for a URL or a secret:
//! `ReadOnly` / `readonly_box`). Values go in and out as the web's strings
//! ("12", "2026-11-20", "09:00", "2026-11-20T09:00"), so an area needs no
//! date types of its own.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use crundi_theme::T;
use gpui::{prelude::FluentBuilder as _, *};
use gpui_component::input::{InputEvent, InputState, NumberInput};
use gpui_component::date_picker::{DatePicker, DatePickerState, DateTime};
use gpui_component::time_field::{TimeField, TimeFieldState, TimePrecision};
use gpui_component::Sizable as _;

pub use gpui_component::date_picker::DatePickerEvent;
pub use gpui_component::time_field::TimeFieldEvent;

// ─── Numbers ───

/// A number box's state: `value` ("" for none), its `min` / `max` (None:
/// unbounded) and `step` (the arrows and ↑ / ↓).
pub fn number_state(window: &mut Window, cx: &mut App, value: &str, min: Option<f64>, max: Option<f64>, step: f64) -> Entity<InputState> {
    let v = value.to_string();
    cx.new(|cx| {
        let mut s = InputState::new(window, cx).step(step).default_value(v);
        if let Some(m) = min { s = s.min(m); }
        if let Some(m) = max { s = s.max(m); }
        s
    })
}

/// The number box (the web's `<input type="number">`: typed, or stepped
/// with its arrows and ↑ / ↓). Its height and text are the text box's.
pub fn number_input(state: &Entity<InputState>) -> NumberInput {
    NumberInput::new(state).with_size(gpui_component::Size::Size(T::INPUT_TEXT)).h(T::INPUT_H)
}

/// A number box's value (None when empty or not a number).
pub fn number_value(state: &Entity<InputState>, cx: &App) -> Option<f64> { state.read(cx).value().trim().parse().ok() }

// ─── Dates and times ───

fn parse_date(v: &str) -> Option<NaiveDate> { NaiveDate::parse_from_str(v.trim(), "%Y-%m-%d").ok() }
fn parse_time(v: &str) -> Option<NaiveTime> {
    let v = v.trim();
    NaiveTime::parse_from_str(v, "%H:%M").or_else(|_| NaiveTime::parse_from_str(v, "%H:%M:%S")).ok()
}

/// A date box's state (the web's `type="date"`), from "YYYY-MM-DD" ("" for none).
pub fn date_state(window: &mut Window, cx: &mut App, value: &str) -> Entity<DatePickerState> {
    let d = parse_date(value);
    cx.new(|cx| {
        let mut s = DatePickerState::new(window, cx).date_format("%Y-%m-%d");
        if let Some(d) = d { s.set_date(d, window, cx); }
        s
    })
}

/// A date-and-time box's state (the web's `type="datetime-local"`), from
/// "YYYY-MM-DDTHH:MM" ("" for none).
pub fn datetime_state(window: &mut Window, cx: &mut App, value: &str) -> Entity<DatePickerState> {
    let v = NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%dT%H:%M").ok();
    cx.new(|cx| {
        let mut s = DatePickerState::new(window, cx).date_format("%Y-%m-%d").time_precision(TimePrecision::Minute);
        if let Some(v) = v { s.set_date_time(v, window, cx); }
        s
    })
}

/// The date (or date-and-time) box: the value, a calendar under it on a click.
pub fn date_input(state: &Entity<DatePickerState>) -> DatePicker {
    DatePicker::new(state).with_size(gpui_component::Size::Size(T::INPUT_TEXT)).h(T::INPUT_H)
}

/// A date box's value: "YYYY-MM-DD" ("" for none).
pub fn date_value(state: &Entity<DatePickerState>, cx: &App) -> String {
    match state.read(cx).date_time() { DateTime::Single(Some(v)) => v.format("%Y-%m-%d").to_string(), _ => String::new() }
}

/// A date-and-time box's value: "YYYY-MM-DDTHH:MM" ("" for none).
pub fn datetime_value(state: &Entity<DatePickerState>, cx: &App) -> String {
    match state.read(cx).date_time() { DateTime::Single(Some(v)) => v.format("%Y-%m-%dT%H:%M").to_string(), _ => String::new() }
}

/// A time box's state (the web's `type="time"`), from "HH:MM" (none: 00:00).
pub fn time_state(window: &mut Window, cx: &mut App, value: &str) -> Entity<TimeFieldState> {
    let t = parse_time(value);
    cx.new(|cx| {
        let mut s = TimeFieldState::new(window, cx).precision(TimePrecision::Minute);
        if let Some(t) = t { s.set_time(t, window, cx); }
        s
    })
}

/// The time box (hours and minutes, typed or stepped with ↑ / ↓).
pub fn time_input(state: &Entity<TimeFieldState>) -> TimeField {
    TimeField::new(state).with_size(gpui_component::Size::Size(T::INPUT_TEXT)).h(T::INPUT_H)
}

/// Put a date in a date box ("YYYY-MM-DD"; "" or a date that does not
/// read empties it).
pub fn set_date(state: &Entity<DatePickerState>, value: &str, window: &mut Window, cx: &mut App) {
    let d = parse_date(value);
    if date_value(state, cx) == d.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default() { return; }
    state.update(cx, |s, cx| match d {
        Some(d) => s.set_date(d, window, cx),
        None => s.set_date(gpui_component::calendar::Date::Single(None), window, cx),
    });
}

/// Put a time in a time box ("HH:MM"; one that does not read leaves it).
pub fn set_time(state: &Entity<TimeFieldState>, value: &str, window: &mut Window, cx: &mut App) {
    let Some(t) = parse_time(value) else { return };
    if time_value(state, cx) == t.format("%H:%M").to_string() { return; }
    state.update(cx, |s, cx| s.set_time(t, window, cx));
}

/// A time box's value: "HH:MM".
pub fn time_value(state: &Entity<TimeFieldState>, cx: &App) -> String { state.read(cx).time().format("%H:%M").to_string() }

// ─── Option buttons ───

/// The web's wrapping option buttons (`.sm-seg` of `.sm-segb`: one on, in
/// the accent; they wrap onto the next line when the row is too narrow).
/// `options`: (value, label); `on`: the value that is on; `off`: the whole
/// row disabled (`.sm-segb[disabled]`).
pub fn option_buttons(id: impl Into<SharedString>, options: &[(&str, &str)], on: &str, off: bool, pick: impl Fn(&str, &mut Window, &mut App) + 'static) -> Div {
    let id: SharedString = id.into();
    let pick = std::rc::Rc::new(pick);
    let mut d = div().flex().flex_wrap().gap(T::SP_6);
    for (i, (value, label)) in options.iter().enumerate() {
        let is_on = *value == on;
        let v = value.to_string();
        let p = pick.clone();
        d = d.child(div().id(ElementId::NamedInteger(id.clone(), i as u64)).px(T::SP_12).py(T::SP_6).rounded(T::R_CONTROL).border_1()
            .text_size(T::TEXT_13)
            .border_color(if is_on { T::accent() } else { T::border() })
            .bg(if is_on { T::accent_dim() } else { T::bg_tertiary() })
            .text_color(if is_on { T::accent_hover() } else { T::text_secondary() })
            .when(off, |d| d.opacity(0.8))
            .when(!off, |d| d.cursor_pointer().on_click(move |_, window, cx| p(&v, window, cx)))
            .child(label.to_string()));
    }
    d
}

// ─── A read-only box ───

/// A read-only text box whose text is selected whole on a click (the web's
/// `readonly onclick="this.select()"`): typing into it changes nothing.
pub struct ReadOnly { state: Entity<InputState>, value: SharedString, _sub: Subscription }

impl ReadOnly {
    pub fn new(window: &mut Window, cx: &mut Context<Self>, value: impl Into<SharedString>) -> Self {
        let value: SharedString = value.into();
        let v = value.clone();
        let state = cx.new(|cx| InputState::new(window, cx).default_value(v));
        let sub = cx.subscribe_in(&state, window, |me: &mut ReadOnly, st, e: &InputEvent, window, cx| {
            if matches!(e, InputEvent::Change) && st.read(cx).value() != me.value {
                let v = me.value.clone();
                st.update(cx, |s, cx| s.set_value(v, window, cx));
            }
        });
        ReadOnly { state, value, _sub: sub }
    }

    /// Show another value.
    pub fn set(&mut self, value: impl Into<SharedString>, window: &mut Window, cx: &mut Context<Self>) {
        let v: SharedString = value.into();
        if v == self.value { return; }
        self.value = v.clone();
        self.state.update(cx, |s, cx| s.set_value(v, window, cx));
        cx.notify();
    }

    pub fn value(&self) -> &SharedString { &self.value }
}

impl Render for ReadOnly {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let st = self.state.clone();
        div().w_full().on_mouse_up(MouseButton::Left, move |_, window, cx| st.update(cx, |s, cx| s.select_all(window, cx)))
            .child(crate::input(&self.state).font_family(T::MONO))
    }
}

/// A read-only box (make it once with `cx.new(|cx| ReadOnly::new(window, cx, value))`).
pub fn readonly_box(e: &Entity<ReadOnly>) -> AnyView { e.clone().into() }

