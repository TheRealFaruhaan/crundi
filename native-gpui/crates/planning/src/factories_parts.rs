//! Factories (4.7, 4.8): what the screens share. The rows the Slint client
//! kept as its PlanningFx* structs (plain Rust here, built by the logic and
//! read by the drawing and the driver's props); the web's small parts
//! (.fx-note / .fx-warn / .fx-err, .ff-field, .ff-issue, the skill chips,
//! .ff-kv); text fields kept per key (their value follows the model unless
//! the person is typing); a size probe; the server calls and timers; and the
//! graph canvas (React Flow's: steps, connections, Controls, MiniMap).

use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

// ─── The rows ───

#[derive(Clone, Default, PartialEq, Debug)]
pub struct Card { pub id: String, pub name: String, pub runnable: bool, pub chip: String, pub desc: String, pub meta: String }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Tpl { pub id: String, pub name: String, pub good_for: String, pub description: String, pub team: String, pub busy: bool }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct RunRow { pub id: String, pub name: String, pub project: String, pub chip: String, pub tone: String, pub task: String, pub meta: String, pub off: bool, pub tag1: String, pub tag2: String }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Clean { pub id: String, pub title: String, pub meta: String, pub why: String, pub warn: String, pub size: String, pub age: String, pub picked: bool }
/// A line: kind note | warn | err | meta | hint | b | open | anyway.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Line { pub kind: String, pub text: String, pub id: String }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Sec { pub id: String, pub label: String, pub hint: String, pub required: bool, pub rows: i32, pub value: String, pub bad: bool }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct FileChip { pub name: String, pub image: bool }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Opt { pub id: String, pub label: String }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Check { pub id: String, pub label: String, pub sub: String, pub on: bool, pub enabled: bool }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Out { pub id: String, pub label: String, pub finish: bool }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Node {
    pub id: String, pub kind: String, pub kind_label: String, pub title: String, pub body: String, pub lines: i32,
    pub x: f32, pub y: f32, pub w: f32, pub h: f32,
    pub sel: bool, pub bad: bool, pub active: bool, pub visited: bool, pub retired: bool,
    pub badge: i32, pub badge_level: String, pub token: String, pub token_kind: String,
    pub doing: bool, pub doing_who: String, pub doing_text: String, pub doing_busy: bool,
    pub skills: String, pub outs: Vec<Out>, pub agent: bool, pub has_in: bool,
}
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Edge { pub id: String, pub path: String, pub arrow: String, pub tone: String, pub sel: bool, pub pulse: bool, pub label: String, pub icon: String, pub lx: f32, pub ly: f32 }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Worker { pub id: String, pub name: String, pub what: String, pub busy: bool, pub blocked: bool, pub chat: String, pub queued: bool }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Kv { pub k: String, pub v: String }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct AskRow { pub token: String, pub prompt: String, pub replies: Vec<Opt> }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Wait { pub token: String, pub head: String, pub gives: String, pub last: String, pub url: String, pub two: bool }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Tl { pub i: i32, pub time: String, pub k: String, pub main: String, pub sub: String, pub on: bool, pub future: bool }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct StepRow { pub who: String, pub num: String, pub summary: String, pub answers: String, pub skills: String }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Issue { pub level: String, pub message: String, pub fix: String, pub key: String }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Person { pub id: String, pub name: String, pub meta: String, pub manager: bool }
#[derive(Clone, Default, PartialEq, Debug)]
pub struct CrSec { pub id: String, pub label: String, pub hint: String, pub required: bool, pub list: bool }

// ─── Small helpers ───


/// encodeURIComponent.
pub fn enc(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~!*'()".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// The server's address (no slash at the end).
pub fn base(cx: &App) -> String { crundi_shell::api(cx).map(|a| a.base().trim_end_matches('/').to_string()).unwrap_or_default() }

pub fn err_text(r: &Value, fallback: &str) -> String { let e = super::model::s(r, "error"); if e.is_empty() { fallback.into() } else { e } }

/// A GET; `done` gets the JSON, or {ok:false, error} when it failed; the window is drawn again after.
pub fn get(cx: &mut App, path: String, done: impl FnOnce(Value, &mut App) + 'static) {
    crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| { done(r.unwrap_or_else(|e| json!({ "ok": false, "error": e })), cx); cx.refresh_windows(); });
}
/// A POST; the same.
pub fn post(cx: &mut App, path: &str, body: Value, done: impl FnOnce(Value, &mut App) + 'static) {
    let path = path.to_string();
    crundi_shell::call(cx, move |api| api.post(&path, &body), move |r, cx| { done(r.unwrap_or_else(|e| json!({ "ok": false, "error": e })), cx); cx.refresh_windows(); });
}

/// After `ms`, on the UI thread (then drawn again).
pub fn after(cx: &mut App, ms: u64, f: impl FnOnce(&mut App) + 'static) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        cx.background_executor().timer(std::time::Duration::from_millis(ms)).await;
        let _ = cx.update(|cx| { f(cx); cx.refresh_windows(); });
    }).detach();
}

/// The window (for toasts and asks from a server call's answer).
/// Deferred: a click or the driver may be inside the window's own update,
/// where the window cannot be borrowed again.
pub fn with_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    cx.defer(move |cx| {
        if let Some(h) = cx.active_window().or_else(|| cx.windows().first().copied()) { let _ = h.update(cx, |_, window, cx| f(window, cx)); }
        cx.refresh_windows();
    });
}
pub fn toast(cx: &mut App, text: &str) { let t = text.to_string(); with_window(cx, move |w, cx| w::toast(w, cx, t, w::Toast::Info)); }
pub fn toast_ok(cx: &mut App, text: &str) { let t = text.to_string(); with_window(cx, move |w, cx| w::toast(w, cx, t, w::Toast::Success)); }
pub fn toast_error(cx: &mut App, text: &str) { let t = text.to_string(); with_window(cx, move |w, cx| w::toast(w, cx, t, w::Toast::Error)); }

/// The web's confirm(): `then(true)` on OK.
pub fn confirm(cx: &mut App, title: &str, message: &str, ok: &str, danger: bool, then: impl FnOnce(bool, &mut App) + 'static) {
    let a = w::Ask { title: title.to_string().into(), message: message.to_string().into(), ok: ok.to_string().into(), danger, prompt: None, placeholder: "".into() };
    let then = Rc::new(RefCell::new(Some(then)));
    with_window(cx, move |window, cx| w::ask_confirm(window, cx, a, move |ok, _, cx| { if let Some(t) = then.borrow_mut().take() { t(ok, cx); } cx.refresh_windows(); }));
}
/// A prompt for a line of text: `then(Some(text))` on OK.
pub fn prompt(cx: &mut App, title: &str, message: &str, value: &str, ok: &str, then: impl FnOnce(Option<String>, &mut App) + 'static) {
    let a = w::Ask { title: title.to_string().into(), message: message.to_string().into(), ok: ok.to_string().into(), danger: false, prompt: Some(value.to_string()), placeholder: "".into() };
    let then = Rc::new(RefCell::new(Some(then)));
    with_window(cx, move |window, cx| w::ask(window, cx, a, move |r, _, cx| { if let Some(t) = then.borrow_mut().take() { t(r, cx); } cx.refresh_windows(); }));
}

/// Small saved settings of the area (the web's localStorage; the Slint client's planning.json).
pub mod prefs {
    use serde_json::Value;
    fn path() -> std::path::PathBuf { crundi_core::store::config_dir().join("planning.json") }
    fn all() -> Value { std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Object(Default::default())) }
    pub fn get(k: &str) -> String { all().get(k).and_then(|v| v.as_str()).unwrap_or("").to_string() }
    pub fn set(k: &str, v: &str) {
        let mut a = all();
        a[k] = Value::String(v.to_string());
        let _ = std::fs::write(path(), a.to_string());
    }
}

// ─── Text fields, one per key ───

type OnText = Rc<dyn Fn(String, &mut App)>;

enum Box_ { Line(Entity<w::InputState>), Area(Entity<w::TextareaState>) }

struct FieldState { b: Box_, synced: String, _sub: Subscription }

thread_local! {
    static FIELDS: RefCell<HashMap<String, FieldState>> = RefCell::new(HashMap::new());
    static ON: RefCell<HashMap<String, OnText>> = RefCell::new(HashMap::new());
}

fn fire(key: &str, v: String, cx: &mut App) {
    FIELDS.with(|f| if let Some(x) = f.borrow_mut().get_mut(key) { x.synced = v.clone(); });
    let on = ON.with(|o| o.borrow().get(key).cloned());
    if let Some(on) = on { on(v, cx); }
    cx.refresh_windows();
}

/// What a field does with its text: on every change, or only on Enter and leaving it.
#[derive(Clone, Copy, PartialEq)]
pub enum Commit { Change, Leave }

/// A one-line field for `key`, showing `value` (the model's) unless typed over.
pub fn line(key: &str, value: &str, placeholder: &str, mono: bool, masked: bool, commit: Commit, window: &mut Window, cx: &mut App, on: impl Fn(String, &mut App) + 'static) -> AnyElement {
    ON.with(|o| o.borrow_mut().insert(key.to_string(), Rc::new(on)));
    let have = FIELDS.with(|f| f.borrow().get(key).map(|x| match &x.b { Box_::Line(e) => Some(e.clone()), _ => None }));
    let e = match have.flatten() {
        Some(e) => e,
        None => {
            let ph = placeholder.to_string();
            let e = cx.new(|cx| { let mut s = w::InputState::new(window, cx).placeholder(ph).masked(masked); s.set_value(value.to_string(), window, cx); s });
            let k = key.to_string();
            let sub = window.subscribe(&e, cx, move |st, ev: &w::InputEvent, _w, cx| {
                let v = st.read(cx).value().to_string();
                match ev {
                    w::InputEvent::Change if commit == Commit::Change => fire(&k, v, cx),
                    w::InputEvent::PressEnter { .. } | w::InputEvent::Blur if commit == Commit::Leave => fire(&k, v, cx),
                    w::InputEvent::PressEnter { .. } => { let on = ON.with(|o| o.borrow().get(&format!("{k}#enter")).cloned()); if let Some(on) = on { on(v, cx); cx.refresh_windows(); } }
                    _ => {}
                }
            });
            FIELDS.with(|f| f.borrow_mut().insert(key.to_string(), FieldState { b: Box_::Line(e.clone()), synced: value.to_string(), _sub: sub }));
            e
        }
    };
    sync(key, value, window, cx);
    let i = w::input(&e);
    let d = div().w_full().when(mono, |d| d.font_family(T::MONO));
    d.child(i).into_any_element()
}

/// Enter in the field `key` (a prompt's Unlock, say).
pub fn on_enter(key: &str, f: impl Fn(String, &mut App) + 'static) { ON.with(|o| o.borrow_mut().insert(format!("{key}#enter"), Rc::new(f))); }

/// A multi-line field (`rows` high at least).
pub fn area(key: &str, value: &str, placeholder: &str, rows: usize, mono: bool, window: &mut Window, cx: &mut App, on: impl Fn(String, &mut App) + 'static) -> AnyElement {
    ON.with(|o| o.borrow_mut().insert(key.to_string(), Rc::new(on)));
    let have = FIELDS.with(|f| f.borrow().get(key).map(|x| match &x.b { Box_::Area(e) => Some(e.clone()), _ => None }));
    let e = match have.flatten() {
        Some(e) => e,
        None => {
            let ph = placeholder.to_string();
            let rows = rows.max(1);
            let e = cx.new(|cx| { let mut s = w::TextareaState::new(window, cx).auto_grow(rows, rows.max(12)).placeholder(ph); s.set_value(value.to_string(), window, cx); s });
            let k = key.to_string();
            let sub = window.subscribe(&e, cx, move |st, ev: &w::InputEvent, _w, cx| {
                if let w::InputEvent::Change = ev { let v = st.read(cx).value().to_string(); fire(&k, v, cx); }
            });
            FIELDS.with(|f| f.borrow_mut().insert(key.to_string(), FieldState { b: Box_::Area(e.clone()), synced: value.to_string(), _sub: sub }));
            e
        }
    };
    sync(key, value, window, cx);
    div().w_full().when(mono, |d| d.font_family(T::MONO)).child(w::textarea(&e)).into_any_element()
}

/// The model's value changed from elsewhere (a reload, undo, another step selected): show it.
fn sync(key: &str, value: &str, window: &mut Window, cx: &mut App) {
    let job = FIELDS.with(|f| {
        let mut f = f.borrow_mut();
        let x = f.get_mut(key)?;
        if x.synced == value { return None; }
        x.synced = value.to_string();
        Some(match &x.b { Box_::Line(e) => Box_::Line(e.clone()), Box_::Area(e) => Box_::Area(e.clone()) })
    });
    let v = value.to_string();
    match job {
        Some(Box_::Line(e)) => e.update(cx, |s, cx| s.set_value(v, window, cx)),
        Some(Box_::Area(e)) => e.update(cx, |s, cx| s.set_value(v, window, cx)),
        None => {}
    }
}

/// Focus the field `key` (a dialog's first box).
pub fn focus(key: &str, window: &mut Window, cx: &mut App) {
    let e = FIELDS.with(|f| f.borrow().get(key).map(|x| match &x.b { Box_::Line(e) => Box_::Line(e.clone()), Box_::Area(e) => Box_::Area(e.clone()) }));
    match e { Some(Box_::Line(e)) => e.update(cx, |s, cx| s.focus(window, cx)), Some(Box_::Area(e)) => e.update(cx, |s, cx| s.focus(window, cx)), None => {} }
}

/// Forget the fields under a prefix (a dialog closed: the next one starts fresh).
pub fn forget(prefix: &str) { FIELDS.with(|f| f.borrow_mut().retain(|k, _| !k.starts_with(prefix))); }

// ─── A size probe ───

thread_local! { static SIZES: RefCell<HashMap<&'static str, Bounds<Pixels>>> = RefCell::new(HashMap::new()); }

/// Where a part was last drawn (window coordinates).
pub fn bounds_of(key: &'static str) -> Option<Bounds<Pixels>> { SIZES.with(|s| s.borrow().get(key).copied()) }

/// Fills its parent (put it in a relative box) and reports the parent's bounds when they change.
pub fn probe(key: &'static str, changed: impl Fn(Bounds<Pixels>, &mut App) + 'static) -> impl IntoElement {
    canvas(move |b, _w, cx| {
        let old = SIZES.with(|s| s.borrow_mut().insert(key, b));
        if old.map(|o| o.size != b.size).unwrap_or(true) {
            let changed = Rc::new(changed);
            cx.defer(move |cx| { changed(b, cx); cx.refresh_windows(); });
        }
    }, |_, _, _, _| {}).absolute().top_0().left_0().size_full()
}

// ─── Small parts ───

/// .fx-note / .fx-warn / .fx-err / .fx-meta / .ff-hint / b: a line of words.
pub fn note(kind: &str, text: impl Into<SharedString>) -> Div {
    let boxed = kind == "warn" || kind == "err";
    let fg = match kind { "warn" => T::yellow(), "err" => T::error_text(), "b" => T::text(), _ => T::text_muted() };
    div().w_full().text_color(fg)
        .text_size(if kind == "meta" || kind == "hint" { T::TEXT_11 } else { T::TEXT_12 })
        .when(kind == "b", |d| d.font_weight(T::W_SEMIBOLD))
        .when(boxed, |d| d.px(T::SP_10).py(T::SP_8).rounded(T::R_CONTROL).bg(if kind == "warn" { T::yellow_dim() } else { T::red_dim() }))
        .child(text.into())
}

/// .ff-field: the label (and a tag after it) over its control, a hint under it.
pub fn field(label: &str, tag: &str, hint: &str, control: impl IntoElement) -> Div {
    div().flex().flex_col().gap(T::SP_4).w_full().min_w_0()
        .child(div().flex().items_center().gap(T::SP_4)
            .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(label.to_string()))
            .when(!tag.is_empty(), |d| d.child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(tag.to_string()))))
        .child(control)
        .when(!hint.is_empty(), |d| d.child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(hint.to_string())))
}

/// .ff-subtitle: small, upper case, muted.
pub fn sub(text: &str) -> Div { div().text_size(T::TEXT_11).text_color(T::text_muted()).font_weight(T::W_MEDIUM).child(text.to_string()) }

/// .ff-issue: an alert icon, the words (error red, warning yellow) and the fix.
pub fn issue(id: impl Into<ElementId>, level: &str, message: &str, fix: &str) -> Stateful<Div> {
    let fg = if level == "error" { T::error_text() } else { T::yellow() };
    div().id(id).flex().gap(T::SP_8).p(T::SP_8).rounded(T::R_CONTROL).w_full()
        .bg(if level == "error" { T::red_dim() } else { T::yellow_dim() })
        .child(div().pt(T::SP_2).text_color(fg).child(w::icon(IconName::TriangleAlert, T::ICON_MD)))
        .child(div().flex().flex_col().flex_1().min_w_0().gap(T::SP_2)
            .child(div().text_size(T::TEXT_12).text_color(fg).child(message.to_string()))
            .when(!fix.is_empty(), |d| d.child(div().text_size(T::TEXT_11).text_color(fg).opacity(T::FACTORIES_O_SOFT).child(fix.to_string()))))
}

/// The skills a text names (.ff-chip ok / warn).
pub fn mentions(items: &[Check]) -> Div {
    let mut d = div().flex().flex_wrap().gap(T::SP_4);
    for m in items { d = d.child(w::icon_tag(if m.on { IconName::Check } else { IconName::CircleAlert }, format!("/{}", m.label), if m.on { "ok" } else { "warn" })); }
    d
}

/// .ff-kv: a muted key, the value beside it.
pub fn kv(k: &str, v: &str) -> Div {
    div().flex().gap(T::SP_10).w_full()
        .child(div().flex_none().w(T::SP_48 + T::SP_24).text_size(T::TEXT_12).text_color(T::text_muted()).child(k.to_string()))
        .child(div().flex_1().min_w_0().overflow_hidden().text_size(T::TEXT_12).text_color(T::text()).child(v.to_string()))
}

/// .fx-chip's tone, as the widgets' tag's.
pub fn tone(t: &str) -> &'static str { match t { "ok" => "ok", "warn" => "warn", "bad" => "error", "info" => "sky", _ => "" } }

/// A run state's look (.ff-runstate).
pub fn state_tone(st: &str) -> &'static str {
    match st { "running" => "ok", "waiting" | "paused" => "warn", "crashed" | "stopped" => "bad", "finishing" | "finished" => "info", _ => "" }
}

/// A button with an icon before its label (.fx-btn): kind "" | primary | ghost | danger | ghost-danger; `fold` keeps the icon only.
pub fn btn(id: impl Into<ElementId>, label: &str, icon: Option<IconName>, kind: &str, enabled: bool, fold: bool) -> gpui_component::button::Button {
    let l = if fold && icon.is_some() { String::new() } else { label.to_string() };
    let b = match kind { "primary" => w::primary(id, l), "ghost" => w::ghost(id, l), "danger" | "ghost-danger" => w::danger(id, l), _ => w::button(id, l) };
    let b = match icon { Some(i) => b.icon(i), None => b };
    let b = if fold { b.tooltip(label.to_string()) } else { b };
    w::disabled(b, !enabled)
}

/// A step's icon by its type (nodes.jsx META).
pub fn kind_icon(k: &str) -> IconName {
    match k { "start" => IconName::Play, "manager" => IconName::Crown, "decision" => IconName::GitBranch, "join" => IconName::GitMerge, "ask" => IconName::MessageCircleQuestionMark, "wait" => IconName::Clock, "end" => IconName::Flag, _ => IconName::User }
}
/// A step's head colour.
pub fn head_color(k: &str) -> Hsla {
    match k { "manager" => T::yellow(), "decision" | "join" => T::sky(), "ask" | "wait" => T::accent_hover(), "start" | "end" => T::green(), _ => T::text_secondary() }
}
pub fn stroke_color(t: &str, sel: bool) -> Hsla {
    if sel { return T::accent(); }
    match t { "bad" => T::red(), "used" => T::green(), "note" => T::yellow(), "loop" => T::sky(), "job" => T::accent_hover(), _ => T::text_secondary() }
}
fn token_color(k: &str) -> Hsla { match k { "blocked" => T::yellow(), "ask" | "wait" => T::accent_hover(), "join" => T::text_secondary(), _ => T::green() } }

// ─── The graph canvas ───

/// What a canvas shows and how it answers.
pub struct Canvas {
    pub which: &'static str,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub zoom: f32, pub pan_x: f32, pub pan_y: f32,
    pub editable: bool,
    pub minimap: bool,
    pub controls_top: bool,
    pub bounds: (f32, f32, f32, f32),
    pub conn: Option<String>,
    pub cb: Rc<CanvasCalls>,
}

/// The canvas's calls (canvas coordinates: from its top left).
pub struct CanvasCalls {
    pub size: Box<dyn Fn(f32, f32, &mut App)>,
    pub pan: Box<dyn Fn(f32, f32, &mut App)>,
    pub zoom: Box<dyn Fn(f32, f32, f32, &mut App)>,
    pub control: Box<dyn Fn(&str, &mut App)>,
    pub mini: Box<dyn Fn(f32, f32, &mut App)>,
    pub pane: Box<dyn Fn(&mut App)>,
    pub node_down: Box<dyn Fn(&str, &mut App)>,
    pub node_drag: Box<dyn Fn(&str, f32, f32, &mut App)>,
    pub node_up: Box<dyn Fn(&str, bool, &mut App)>,
    pub handle_drag: Box<dyn Fn(&str, &str, f32, f32, &mut App)>,
    pub handle_up: Box<dyn Fn(&str, &str, f32, f32, &mut App)>,
    pub edge: Box<dyn Fn(&str, &mut App)>,
    pub key: Box<dyn Fn(&str, &mut App)>,
}

/// The pointer's job while the button is down.
#[derive(Clone, Default)]
enum Drag { #[default] None, Pan(Point<Pixels>, bool), Node(String, Point<Pixels>, bool), Handle(String, String), Mini }

thread_local! {
    static DRAG: RefCell<HashMap<&'static str, Drag>> = RefCell::new(HashMap::new());
    static FOCUS: RefCell<HashMap<&'static str, FocusHandle>> = RefCell::new(HashMap::new());
}
fn drag(which: &'static str) -> Drag { DRAG.with(|d| d.borrow().get(which).cloned().unwrap_or_default()) }
fn set_drag(which: &'static str, x: Drag) { DRAG.with(|d| { d.borrow_mut().insert(which, x); }); }

fn origin(which: &'static str) -> Point<Pixels> { bounds_of(if which == "v" { "cv-v" } else { "cv-e" }).map(|b| b.origin).unwrap_or_default() }
fn local(which: &'static str, p: Point<Pixels>) -> (f32, f32) { let o = origin(which); (f32::from(p.x - o.x), f32::from(p.y - o.y)) }

/// An SVG path (M, L, C, Q, Z in flow units) as GPUI's, placed by `f`.
fn path_of(d: &str, f: &dyn Fn(f32, f32) -> Point<Pixels>, mut pb: PathBuilder) -> Option<Path<Pixels>> {
    let toks: Vec<&str> = d.split_whitespace().collect();
    let num = |i: usize| toks.get(i).and_then(|t| t.parse::<f32>().ok()).unwrap_or(0.0);
    let mut i = 0;
    while i < toks.len() {
        match toks[i] {
            "M" => { pb.move_to(f(num(i + 1), num(i + 2))); i += 3; }
            "L" => { pb.line_to(f(num(i + 1), num(i + 2))); i += 3; }
            "Q" => { pb.curve_to(f(num(i + 3), num(i + 4)), f(num(i + 1), num(i + 2))); i += 5; }
            "C" => { pb.cubic_bezier_to(f(num(i + 5), num(i + 6)), f(num(i + 1), num(i + 2)), f(num(i + 3), num(i + 4))); i += 7; }
            "Z" => { pb.close(); i += 1; }
            _ => i += 1,
        }
    }
    pb.build().ok()
}

pub fn canvas_view(c: Canvas, window: &mut Window, cx: &mut App) -> AnyElement {
    let which = c.which;
    let (z, pxx, pyy) = (c.zoom, c.pan_x, c.pan_y);
    let fh = FOCUS.with(|f| f.borrow_mut().entry(which).or_insert_with(|| cx.focus_handle()).clone());
    let cb = c.cb.clone();
    let probe_key: &'static str = if which == "v" { "cv-v" } else { "cv-e" };
    let mut root = div().id(("fx-canvas", if which == "v" { 0usize } else { 1 })).relative().size_full().min_h_0().min_w_0().overflow_hidden().bg(T::bg())
        .track_focus(&fh)
        .child({ let cb = cb.clone(); probe(probe_key, move |b, cx| (cb.size)(f32::from(b.size.width), f32::from(b.size.height), cx)) });
    // The background: pan, click to deselect, the wheel zooms, Delete removes.
    {
        let cb1 = cb.clone();
        let fh2 = fh.clone();
        root = root
            .on_mouse_down(MouseButton::Left, move |e, window, cx| { set_drag(which, Drag::Pan(e.position, false)); window.focus(&fh2, cx); })
            .on_scroll({ let cb = cb.clone(); move |e, window, cx| {
                let d = e.delta.pixel_delta(window.line_height());
                let (mx, my) = local(which, e.position);
                (cb.zoom)(f32::from(d.y), mx, my, cx);
                cx.refresh_windows();
            } })
            // A touchpad's pinch zooms about its centre (the owner, 18 Nov): the
            // wheel's zoom is exp(d × 0.0025), so a pinch's scale is that d.
            .on_pinch({ let cb = cb.clone(); move |e, _, cx| {
                let (mx, my) = local(which, e.position);
                (cb.zoom)((1.0 + e.delta).max(0.05).ln() / 0.0025, mx, my, cx);
                cx.stop_propagation();
                cx.refresh_windows();
            } })
            .on_key_down({ let cb = cb.clone(); move |e, _w, cx| {
                let k = e.keystroke.key.as_str();
                if k == "delete" || k == "backspace" { (cb.key)("delete", cx); cx.refresh_windows(); } else if k == "escape" { (cb.key)("escape", cx); cx.refresh_windows(); }
            } })
            .on_mouse_move(move |e, _w, cx| {
                if e.pressed_button != Some(MouseButton::Left) { return; }
                match drag(which) {
                    Drag::Pan(p, _) => { let dx = f32::from(e.position.x - p.x); let dy = f32::from(e.position.y - p.y); set_drag(which, Drag::Pan(e.position, true)); (cb1.pan)(dx, dy, cx); cx.refresh_windows(); }
                    Drag::Node(id, p, _) => { let dx = f32::from(e.position.x - p.x); let dy = f32::from(e.position.y - p.y); set_drag(which, Drag::Node(id.clone(), p, true)); (cb1.node_drag)(&id, dx, dy, cx); cx.refresh_windows(); }
                    Drag::Handle(n, h) => { let (x, y) = local(which, e.position); (cb1.handle_drag)(&n, &h, x, y, cx); cx.refresh_windows(); }
                    _ => {}
                }
            })
            .on_mouse_up(MouseButton::Left, { let cb = cb.clone(); move |e, _w, cx| {
                match drag(which) {
                    Drag::Pan(_, moved) => { if !moved { (cb.pane)(cx); } }
                    Drag::Node(id, _, moved) => (cb.node_up)(&id, moved, cx),
                    Drag::Handle(n, h) => { let (x, y) = local(which, e.position); (cb.handle_up)(&n, &h, x, y, cx); }
                    _ => {}
                }
                set_drag(which, Drag::None);
                cx.refresh_windows();
            } });
    }
    // Connections, under the steps.
    let edges = c.edges.clone();
    let conn = c.conn.clone();
    let ow = which;
    root = root.child(canvas(|_, _, _| {}, move |_b, _, window, _| {
        let o = origin(ow);
        let f = move |x: f32, y: f32| point(o.x + px(x * z + pxx), o.y + px(y * z + pyy));
        for e in &edges {
            let wdt = if e.sel || e.pulse { T::SP_2 + px(1.) } else { T::SP_2 } * z;
            let mut pb = PathBuilder::stroke(wdt);
            if e.tone == "note" || e.tone == "job" { pb = pb.dash_array(&[T::SP_6 * z, T::SP_4 * z]); }
            if let Some(p) = path_of(&e.path, &f, pb) { window.paint_path(p, stroke_color(&e.tone, e.sel)); }
            if let Some(p) = path_of(&e.arrow, &f, PathBuilder::fill()) { window.paint_path(p, stroke_color(&e.tone, e.sel)); }
        }
        if let Some(cp) = &conn { if let Some(p) = path_of(cp, &f, PathBuilder::stroke(T::SP_2 * z)) { window.paint_path(p, T::accent()); } }
    }).absolute().top_0().left_0().size_full());
    // The steps.
    for (i, n) in c.nodes.iter().enumerate() { root = root.child(node_view(i, n, &c, window, cx)); }
    // Edge labels: a click picks the connection.
    for (i, e) in c.edges.iter().enumerate() {
        let has = !e.label.is_empty();
        if !has && !c.editable { continue; }
        let id = e.id.clone();
        let cbe = cb.clone();
        let (lx, ly) = (e.lx * z + pxx, e.ly * z + pyy);
        let border = if e.sel { T::accent() } else if e.tone == "bad" { T::red() } else if e.tone == "note" { T::factories_yellow_edge() } else { T::border() };
        let el = if has {
            div().id(("fx-elabel", i)).absolute().left(px(lx)).top(px(ly)).flex().items_center().gap(T::SP_4 * z).h(T::SP_20 * z).px(T::SP_6 * z)
                .rounded(T::R_PILL).bg(T::bg_secondary()).border_1().border_color(border)
                .text_size(T::TEXT_10 * z).text_color(if e.sel { T::text() } else if e.tone == "bad" { T::error_text() } else if e.tone == "note" { T::yellow() } else { T::text_secondary() })
                .when(!e.icon.is_empty(), |d| d.child(w::icon(if e.icon == "loop" { IconName::Repeat } else { IconName::StickyNote }, T::ICON_SM * z)))
                .child(e.label.clone())
        } else {
            div().id(("fx-elabel", i)).absolute().left(px(lx)).top(px(ly)).size(T::SP_10 * z).rounded(T::R_PILL)
                .bg(if e.sel { T::accent() } else { T::border() }).hover(|d| d.bg(T::accent()))
        };
        root = root.child(el.cursor_pointer().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, _, cx| { (cbe.edge)(&id, cx); cx.refresh_windows(); }));
    }
    // Controls: zoom in, out, fit (bottom left; top left on a narrow editor).
    let ctl = |id: &'static str, icon: IconName, tip: &'static str, k: &'static str| {
        let cb = cb.clone();
        w::icon_button(id, icon, tip).on_click(move |_, _, cx| { (cb.control)(k, cx); cx.refresh_windows(); })
    };
    let controls = div().absolute().left(T::SP_12).when(c.controls_top, |d| d.top(T::SP_12)).when(!c.controls_top, |d| d.bottom(T::SP_12))
        .flex().flex_col().rounded(T::R_FIELD).border_1().border_color(T::border()).bg(T::bg_card())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(ctl(if which == "v" { "v-zoom-in" } else { "e-zoom-in" }, IconName::Plus, "Zoom in", "zoom-in"))
        .child(ctl(if which == "v" { "v-zoom-out" } else { "e-zoom-out" }, IconName::Minus, "Zoom out", "zoom-out"))
        .child(ctl(if which == "v" { "v-fit" } else { "e-fit" }, IconName::Maximize, "Fit view", "fit"));
    root = root.child(controls);
    // The minimap: the steps in small, the view's frame; a click moves the view there.
    let (bx, by, bw, bh) = c.bounds;
    if c.minimap && bw > 0.0 {
        let (mw, mh) = (f32::from(T::FACTORIES_MINI_W), f32::from(T::FACTORIES_MINI_H));
        let s = (mw / bw).min(mh / bh.max(1.0));
        let (ox, oy) = ((mw - bw * s) / 2.0, (mh - bh * s) / 2.0);
        let (cw, ch) = bounds_of(probe_key).map(|b| (f32::from(b.size.width), f32::from(b.size.height))).unwrap_or((0.0, 0.0));
        let mut m = div().id(("fx-mini", if which == "v" { 0usize } else { 1 })).absolute().right(T::SP_12).bottom(T::SP_12).w(T::FACTORIES_MINI_W).h(T::FACTORIES_MINI_H)
            .rounded(T::R_FIELD).border_1().border_color(T::border()).bg(T::bg_secondary()).overflow_hidden().cursor_pointer();
        for n in &c.nodes {
            m = m.child(div().absolute().left(px(ox + (n.x - bx) * s)).top(px(oy + (n.y - by) * s)).w(px(n.w * s)).h(px(n.h * s)).rounded(T::SP_2)
                .bg(if n.bad { T::red() } else if n.active { T::green() } else if n.visited { T::factories_green_ring() } else { T::factories_slate_edge() }));
        }
        m = m.child(div().absolute().left(px(ox + (-pxx / z - bx) * s)).top(px(oy + (-pyy / z - by) * s)).w(px(cw / z * s)).h(px(ch / z * s))
            .border_1().border_color(T::accent()).bg(T::accent_dim()));
        let cbm = cb.clone();
        let at = move |p: Point<Pixels>| -> (f32, f32) {
            let o = bounds_of(probe_key).map(|b| b.origin + point(b.size.width - T::SP_12 - T::FACTORIES_MINI_W, b.size.height - T::SP_12 - T::FACTORIES_MINI_H)).unwrap_or_default();
            (bx + (f32::from(p.x - o.x) - ox) / s, by + (f32::from(p.y - o.y) - oy) / s)
        };
        m = m.on_mouse_down(MouseButton::Left, move |e, _, cx| { cx.stop_propagation(); set_drag(which, Drag::Mini); let (x, y) = at(e.position); (cbm.mini)(x, y, cx); cx.refresh_windows(); })
            .on_mouse_move({ let cb = cb.clone(); move |e, _, cx| { if e.pressed_button == Some(MouseButton::Left) && matches!(drag(which), Drag::Mini) { let (x, y) = at(e.position); (cb.mini)(x, y, cx); cx.refresh_windows(); } } });
        root = root.child(m);
    }
    root.into_any_element()
}

/// One step (.ff-node), at the canvas's zoom.
fn node_view(i: usize, n: &Node, c: &Canvas, _window: &mut Window, _cx: &mut App) -> AnyElement {
    let z = c.zoom;
    let which = c.which;
    let (x, y) = (n.x * z + c.pan_x, n.y * z + c.pan_y);
    let border = if n.sel { T::accent() } else if n.active { T::green() } else if n.bad { T::red() } else if n.visited { T::factories_green_ring() } else { T::border() };
    let id = n.id.clone();
    let cb = c.cb.clone();
    let editable = c.editable;
    let pad = T::SP_10 * z;
    let body_h = T::SP_16 * z * n.lines as f32;
    let mut card = div().id(("fx-node", i)).absolute().left(px(x)).top(px(y)).w(px(n.w * z)).h(px(n.h * z))
        .when(n.retired, |d| d.opacity(T::FACTORIES_O_SOFT))
        .when(n.active || n.sel, |d| d.shadow(vec![BoxShadow { color: if n.sel { T::factories_accent_ring() } else { T::factories_green_ring() }, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: T::SP_4 * z, inset: false }]))
        .rounded(T::R_CARD * z).bg(T::bg_card()).border_1().border_color(border)
        .cursor(if editable { CursorStyle::OpenHand } else { CursorStyle::PointingHand })
        .child(div().absolute().left(pad).top(pad).right(pad).h(T::SP_16 * z).flex().items_center().gap(T::SP_6 * z).text_color(head_color(&n.kind))
            .child(w::icon(kind_icon(&n.kind), T::ICON_MD * z))
            .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_10 * z).child(n.kind_label.clone())))
        .child(div().absolute().left(pad).right(pad).top((T::SP_10 + T::SP_16 + T::SP_2) * z).h(T::SP_20 * z).flex().items_center()
            .text_size(T::TEXT_13 * z).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(div().truncate().child(n.title.clone())));
    let body_top = (T::SP_10 + T::SP_16 + T::SP_2 + T::SP_20) * z;
    card = if n.doing {
        card.child(div().absolute().left(pad).right(pad).top(body_top).h(body_h).flex().gap(T::SP_6 * z).overflow_hidden()
            .child(div().mt(T::SP_4 * z).flex_none().size(T::SP_8 * z).rounded(T::R_PILL).bg(if n.doing_busy { T::green() } else { T::text_muted() }))
            .child(div().flex_1().min_w_0().text_size(T::TEXT_11 * z).line_height(T::SP_16 * z).text_color(T::text_secondary()).child(format!("{} {}", n.doing_who, n.doing_text))))
    } else {
        card.child(div().absolute().left(pad).right(pad).top(body_top).h(body_h).overflow_hidden().text_size(T::TEXT_11 * z).line_height(T::SP_16 * z).text_color(T::text_muted()).child(n.body.clone()))
    };
    if !n.skills.is_empty() {
        card = card.child(div().absolute().left(pad).right(pad).top(body_top + T::SP_8 * z + body_h).h(T::SP_20 * z).flex().items_center()
            .text_size(T::TEXT_10 * z).font_family(T::MONO).text_color(T::accent_hover()).child(div().truncate().child(n.skills.clone())));
    }
    let outs = n.outs.len();
    if outs > 0 {
        card = card.child(div().absolute().left_0().right_0().bottom(T::SP_32 * z).h(px(1.)).bg(T::border()));
        let mut row = div().absolute().left_0().right_0().bottom_0().h(T::SP_32 * z - T::SP_4 * z).flex();
        for o in &n.outs { row = row.child(div().flex_1().min_w_0().flex().justify_center().text_size(T::TEXT_10 * z).text_color(if o.finish { T::green() } else { T::text_secondary() }).child(div().truncate().child(o.label.clone()))); }
        card = card.child(row);
    }
    // Handles: in (top), outputs (bottom), the note handles at a person's sides.
    let hs = T::SP_10 * z;
    let ring = |c: Hsla| div().absolute().size(hs).rounded(T::R_PILL).bg(T::bg_card()).border(T::SP_2 * z).border_color(c);
    if n.has_in { card = card.child(ring(T::text_secondary()).left(px(n.w * z / 2.) - hs / 2.).top(-hs / 2.)); }
    if n.agent {
        card = card.child(ring(T::yellow()).left(-hs / 2.).top(px(n.h * z / 2.) - hs / 2.));
        card = card.child(handle(i * 100 + 99, which, &n.id, "note", T::yellow(), editable, cb.clone()).left(px(n.w * z) - hs / 2.).top(px(n.h * z / 2.) - hs / 2.).size(hs).border(T::SP_2 * z));
    }
    for (j, o) in n.outs.iter().enumerate() {
        let cx0 = n.w * z * (2 * j + 1) as f32 / (2 * outs) as f32;
        card = card.child(handle(i * 100 + j, which, &n.id, &o.id, if o.finish { T::green() } else { T::text_secondary() }, editable, cb.clone()).left(px(cx0) - hs / 2.).top(px(n.h * z) - hs / 2.).size(hs).border(T::SP_2 * z));
    }
    // The issues badge and the token pill.
    if n.badge > 0 {
        let fg = if n.badge_level == "error" { T::white() } else { T::bg() };
        card = card.child(div().absolute().right(-T::SP_8 * z).top(-T::SP_8 * z).h(T::SP_16 * z).px(T::SP_4 * z).flex().items_center().gap(T::SP_2 * z).rounded(T::R_PILL)
            .bg(if n.badge_level == "error" { T::red() } else { T::yellow() }).text_color(fg).text_size(T::TEXT_10 * z).font_weight(T::W_BOLD)
            .child(w::icon(IconName::TriangleAlert, T::ICON_SM * z)).child(n.badge.to_string()));
    }
    if !n.token.is_empty() {
        card = card.child(div().absolute().left(pad).top(-T::SP_10 * z).h(T::SP_16 * z).px(T::SP_6 * z).flex().items_center().rounded(T::R_PILL)
            .bg(token_color(&n.token_kind)).text_color(T::bg()).text_size(T::TEXT_10 * z).font_weight(T::W_BOLD).child(n.token.clone()));
    }
    let cbd = cb.clone();
    let id2 = id.clone();
    card.on_mouse_down(MouseButton::Left, move |e, _, cx| {
        cx.stop_propagation();
        set_drag(which, Drag::Node(id2.clone(), e.position, false));
        if editable { (cbd.node_down)(&id2, cx); cx.refresh_windows(); }
    }).into_any_element()
}

/// A source handle: drag it to a step to connect (the editor).
fn handle(i: usize, which: &'static str, node: &str, h: &str, tone: Hsla, editable: bool, _cb: Rc<CanvasCalls>) -> Stateful<Div> {
    let (node, h) = (node.to_string(), h.to_string());
    div().id(("fx-handle", i)).absolute().rounded(T::R_PILL).bg(T::bg_card()).border_color(tone)
        .when(editable, |d| d.cursor_crosshair().hover(|d| d.bg(T::accent()).border_color(T::accent())))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| { if editable { cx.stop_propagation(); set_drag(which, Drag::Handle(node.clone(), h.clone())); } })
}
