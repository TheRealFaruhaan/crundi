//! 2.12b: the secret-request dock (sreq-dock): Run a command with X, Approve / Reject, PIN or passphrase, Allow for this chat with a duration.
//! Spec: native-gpui/specs/secret-requests.md; the Slint client's app.rs
//! (sreq_*), ui/sreq.slint; the web's src/webapp-html.js renderSecretPrompt.
//!
//! When Claude's secret_get / secret_run waits on the owner, a card per
//! request at the window's bottom right, over the panes, wherever the person
//! is (the Slint client's and the web's place). The list is the server's
//! (GET /api/secrets at sign-in and after a reconnect, then the
//! `secret-requests` event); one card holds the field (the armed one). The
//! state is one Global (`Sreq`).
//!
//! Where it is drawn: GPUI has no window-wide layer a part can add to, so one
//! composer draws the dock, anchored to the window (deferred, over
//! everything but dialogs): the focused chat's, else the first chat pane's.
//! While the workbench tab is not shown no composer is drawn, so neither is
//! the dock (a gap until the shell offers an overlay hook: shared/requests.md).
//!
//! This file registers its own part ("sreq": the events, the driver's sreq-*
//! actions, the props sreq / sreqFocused / dock) from `init` (lib.rs).

use crate::{secrets, ComposerView};
use crundi_core::{secrets as rules, sreq::{self, Req}};
use crundi_shell::{Area, AreaEvent};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};

actions!(secret_requests, [SreqEsc]);

const DURATIONS: [(&str, &str); 4] = [("15m", "15 minutes"), ("1h", "1 hour"), ("8h", "8 hours"), ("chat", "Until this chat ends")];
/// The window width at which the dock spans the window (the web's 640).
const NARROW: f32 = 640.;

/// This feature's state inside one chat's composer (none: the dock is the window's).
#[derive(Default)]
pub struct State {}

/// The requests waiting on the owner, which card holds the field, and that
/// card's state. What is typed lives in the field only while its card is armed.
pub struct Sreq {
    list: Vec<Req>,
    armed: Option<String>,
    allow_on: bool,
    /// The last duration chosen (kept across cards while the app runs).
    duration: String,
    err: String,
    busy: bool,
    /// The armed card's field (made when drawn), its lock kind, its events.
    input: Option<(Entity<w::InputState>, String, Subscription)>,
    /// The field takes the focus once drawn (Approve on another card).
    focus_pending: bool,
    /// The dock's place on the last frame (the driver's `dock`).
    bounds: Option<Bounds<Pixels>>,
}

impl Default for Sreq {
    fn default() -> Self {
        Sreq { list: vec![], armed: None, allow_on: false, duration: "1h".into(), err: String::new(), busy: false, input: None, focus_pending: false, bounds: None }
    }
}
impl Global for Sreq {}

fn g(cx: &mut App) -> &mut Sreq {
    if !cx.has_global::<Sreq>() { cx.set_global(Sreq::default()); }
    cx.global_mut::<Sreq>()
}

/// A new card holds the field: nothing typed, unticked, no error. The old
/// field is overwritten and dropped.
fn arm(id: Option<String>, focus: bool, cx: &mut App) {
    let r = g(cx);
    let old = if r.armed != id { r.input.take() } else { None };
    r.armed = id;
    r.allow_on = false;
    r.err.clear();
    r.focus_pending = focus;
    if let Some((input, _, _)) = old {
        secrets::with_window(cx, move |window, cx| input.update(cx, |s, cx| s.set_value("", window, cx)));
    }
}

/// The server's whole list (start, or its `secret-requests` event). The armed
/// card stays armed while its request is there (what is typed, the tick and
/// the error with it); else the first is armed, without the focus.
fn set_list(v: &Value, cx: &mut App) {
    let list = sreq::parse(v);
    match sreq::armed(&list, g(cx).armed.as_deref()) {
        Some((_, true)) => {}
        Some((id, false)) => arm(Some(id), false, cx),
        None => arm(None, false, cx),
    }
    g(cx).list = list;
    cx.refresh_windows();
}

/// The list at sign-in and after a reconnect.
fn load(cx: &mut App) {
    crundi_shell::call(cx, |api| api.get("/api/secrets"), |r, cx| {
        if let Ok(v) = r { set_list(v.get("requests").unwrap_or(&Value::Null), cx); }
    });
}

fn on_event(e: &AreaEvent, cx: &mut App) {
    match e {
        AreaEvent::Server("secret-requests", v) => set_list(v.get("requests").unwrap_or(&Value::Null), cx),
        AreaEvent::Connected(true) if crundi_shell::signed_in(cx) => load(cx),
        _ => {}
    }
}

fn armed_req(cx: &App) -> Option<Req> {
    let r = cx.try_global::<Sreq>()?;
    let id = r.armed.as_deref()?;
    r.list.iter().find(|q| q.id == id).cloned()
}

fn typed(cx: &App) -> String {
    cx.try_global::<Sreq>().and_then(|r| r.input.as_ref()).map(|(i, _, _)| i.read(cx).value().to_string()).unwrap_or_default()
}

/// The armed field: focused, its text selected (a check that failed, a refusal, the driver).
fn focus_field(window: &mut Window, cx: &mut App) {
    match cx.try_global::<Sreq>().and_then(|r| r.input.as_ref()).map(|(i, _, _)| i.clone()) {
        Some(i) => i.update(cx, |s, cx| { s.focus(window, cx); s.select_all(window, cx); }),
        None => g(cx).focus_pending = true,
    }
    cx.refresh_windows();
}

/// Approve on a card without the field: it takes the field, and the focus.
fn arm_by_click(id: &str, cx: &mut App) {
    let r = g(cx);
    if r.list.iter().any(|q| q.id == id) && r.armed.as_deref() != Some(id) { arm(Some(id.to_string()), true, cx); }
    cx.refresh_windows();
}

/// Allow for this chat is offered for this request.
fn can_allow(q: &Req, cx: &App) -> bool {
    sreq::can_allow(q, secrets::cap(cx), secrets::collaborator(&q.session, cx))
}

/// Approve on the armed card (or Enter in its field): the local check, then
/// the server. A refusal keeps the text (focused, selected) and says why.
fn confirm(window: &mut Window, cx: &mut App) {
    if g(cx).busy { return; }
    let Some(q) = armed_req(cx) else { return };
    let pin = typed(cx);
    if let Err(e) = sreq::check(&q.lock, &pin) {
        g(cx).err = e.to_string();
        focus_field(window, cx);
        return;
    }
    let allow = (g(cx).allow_on && can_allow(&q, cx)).then(|| g(cx).duration.clone());
    let body = sreq::approve_body(&q, &pin, allow.as_deref());
    g(cx).busy = true;
    cx.refresh_windows();
    let Some(api) = crundi_shell::api(cx) else { return };
    let (id, allow) = (q.id.clone(), allow.is_some());
    crundi_shell::run(cx, move || {
        let mut body = body;
        let r = secrets::post_raw(&api, "/api/secrets", &body);
        // The lock in the body: overwritten once sent.
        if let Some(p) = body.get_mut("pin") { *p = json!(""); }
        r
    }, move |r, cx| {
        g(cx).busy = false;
        match sreq::answer(r, allow) {
            Ok(msg) => {
                // The next card is armed (not focused): the server's event takes this one away.
                let next = g(cx).list.iter().find(|q| q.id != id).map(|q| q.id.clone());
                if g(cx).armed.as_deref() == Some(id.as_str()) { arm(next, false, cx); }
                g(cx).list.retain(|q| q.id != id);
                secrets::toast(cx, msg, w::Toast::Success);
            }
            Err(e) => {
                if g(cx).armed.as_deref() == Some(id.as_str()) {
                    g(cx).err = e;
                    secrets::with_window(cx, focus_field);
                }
            }
        }
        cx.refresh_windows();
    });
}

/// Reject, on any card: at once, no confirmation. The server's event takes
/// the card away; a failure says so (the web says nothing).
fn reject(id: String, cx: &mut App) {
    let Some(api) = crundi_shell::api(cx) else { return };
    if g(cx).armed.as_deref() == Some(id.as_str()) { g(cx).err.clear(); }
    let body = json!({ "action": "deny", "reqId": id });
    crundi_shell::run(cx, move || secrets::post_raw(&api, "/api/secrets", &body), move |r, cx| match r {
        Ok((_, v)) if v.get("ok").and_then(|o| o.as_bool()) != Some(false) => {
            if g(cx).armed.as_deref() == Some(id.as_str()) {
                let next = g(cx).list.iter().find(|q| q.id != id).map(|q| q.id.clone());
                arm(next, false, cx);
            }
            g(cx).list.retain(|q| q.id != id);
            secrets::toast(cx, "Rejected", w::Toast::Info);
            cx.refresh_windows();
        }
        Ok((_, v)) => secrets::toast(cx, v.get("error").and_then(|e| e.as_str()).unwrap_or("Could not reject it"), w::Toast::Error),
        Err(e) => secrets::toast(cx, &e, w::Toast::Error),
    });
}

/// Esc in the field: the error goes, the text stays, the field lets go.
fn escape(window: &mut Window, cx: &mut App) {
    g(cx).err.clear();
    window.blur(cx);
    cx.refresh_windows();
}

// ─── Drawing ───

/// The armed card's field, made for its lock (placeholder, digits for a PIN).
fn ensure_input(window: &mut Window, cx: &mut App) -> Option<Entity<w::InputState>> {
    let q = armed_req(cx)?;
    if let Some((i, lock, _)) = cx.try_global::<Sreq>().and_then(|r| r.input.as_ref()) { if *lock == q.lock { return Some(i.clone()); } }
    let pin = q.lock == "pin";
    let input = cx.new(|cx| w::InputState::new(window, cx).masked(true).placeholder(if pin { "PIN" } else { "Passphrase" }));
    let lock = q.lock.clone();
    let sub = window.subscribe(&input, cx, move |input, ev: &w::InputEvent, window, cx| match ev {
        w::InputEvent::PressEnter { .. } => confirm(window, cx),
        w::InputEvent::Change => {
            let text = input.read(cx).value().to_string();
            let kept = rules::typed(&lock, &text);
            if kept != text { input.update(cx, |s, cx| s.set_value(kept, window, cx)); }
            cx.refresh_windows();
        }
        _ => {}
    });
    g(cx).input = Some((input.clone(), q.lock.clone(), sub));
    Some(input)
}

/// This composer draws the dock: the focused pane's chat, else the first chat pane.
pub(crate) fn is_host(me: &str, cx: &App) -> bool {
    let Some(wb) = crundi_workbench::workbench(cx) else { return false };
    let wbr = wb.read(cx);
    let Some(m) = wbr.layouts.get(&wbr.project) else { return false };
    let keys = m.keys();
    let live = |k: &String| k.strip_prefix("live:").filter(|id| crate::composer(cx, id).is_some()).map(String::from);
    let focused = keys.iter().find(|k| m.leaf_of(k) == Some(wbr.focused)).and_then(live);
    focused.or_else(|| keys.iter().find_map(live)).as_deref() == Some(me)
}

fn card(i: usize, q: &Req, armed: bool, field: Option<Entity<w::InputState>>, cx: &App) -> Div {
    let r = cx.global::<Sreq>();
    let head = div().flex().flex_wrap().items_center().gap(T::SP_6).text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text())
        .child(div().text_size(T::ICON_MD).text_color(T::accent_hover()).child(IconName::Lock))
        .child(if q.run { "Run a command with" } else { "Release" })
        .child(div().font_family(T::MONO).text_color(T::accent_hover()).child(q.name.clone()));
    let why = |t: String| div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(t);
    let mut c = div().flex().flex_col().gap(T::SP_8).py(T::SP_12).px(T::SP_16).rounded(T::R_CARD).border_1().border_color(T::accent()).bg(T::bg_card()).shadow_lg()
        .child(head)
        .when(!q.project.is_empty(), |c| c.child(why(format!("project: {}", q.project))))
        .when(!q.reason.is_empty(), |c| c.child(why(q.reason.clone())));
    if q.run {
        c = c.child(div().id(("sreq-cmd", i)).max_h(T::SREQ_CMD_MAX_H).overflow_y_scroll().p(T::SP_8).rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg())
                .font_family(T::MONO).text_size(T::TEXT_11).line_height(relative(T::LH_CHAT)).text_color(T::text()).child(q.command.clone()))
            .child(div().font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child(format!("the value is bound to ${} and never shown to Claude", q.env)));
    } else {
        c = c.child(why("Claude will be able to read this value.".into()));
    }
    let id = q.id.clone();
    let rid = q.id.clone();
    match field.filter(|_| armed) {
        Some(f) => {
            let busy = r.busy;
            c = c.child(div().pt(T::SP_2).flex().items_center().gap(T::SP_6)
                    .child(div().flex_1().min_w_0().key_context("SreqField").on_action(|_: &SreqEsc, window, cx| escape(window, cx)).child(w::input(&f)))
                    .child(w::disabled(w::primary(("sreq-approve", i), if busy { "Approving…" } else { "Approve" }), busy).on_click(|_, window, cx| confirm(window, cx)))
                    .child(w::button(("sreq-reject", i), "Reject").on_click(move |_, _, cx| reject(rid.clone(), cx))));
            if can_allow(q, cx) {
                let on = r.allow_on;
                let dur = DURATIONS.iter().position(|d| d.0 == r.duration).unwrap_or(1);
                c = c.child(div().pt(T::SP_6).flex().flex_wrap().items_center().gap(T::SP_8).text_size(T::TEXT_13)
                        .child(w::check(("sreq-allow", i), "Allow for this chat", on, |_, _, cx| { let r = g(cx); r.allow_on = !r.allow_on; cx.refresh_windows(); }))
                        .child(w::dropdown(("sreq-dur", i), &DURATIONS.map(|d| d.1), dur, !on, |k, _, cx| { g(cx).duration = DURATIONS[k].0.into(); cx.refresh_windows(); })))
                    .when(on, |c| c.child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("This chat may then run commands with this secret without asking, until then. Its value is never shown.")));
            }
            if !r.err.is_empty() { c = c.child(div().text_size(T::TEXT_12).text_color(T::red()).child(r.err.clone())); }
        }
        None => {
            c = c.child(div().pt(T::SP_2).flex().items_center().gap(T::SP_6)
                .child(w::primary(("sreq-approve", i), "Approve").on_click(move |_, _, cx| arm_by_click(&id, cx)))
                .child(w::button(("sreq-reject", i), "Reject").on_click(move |_, _, cx| reject(rid.clone(), cx))));
        }
    }
    c
}

/// The dock, drawn by one composer (see the head of the file), anchored to
/// the window's bottom right, over the panes and the composers.
pub fn render(c: &mut ComposerView, window: &mut Window, cx: &mut Context<ComposerView>) -> Option<AnyElement> {
    if cx.try_global::<Sreq>().is_none_or(|r| r.list.is_empty()) { return None; }
    let me = c.chat_id(cx);
    if !is_host(&me, cx) { return None; }
    let field = ensure_input(window, cx);
    if g(cx).focus_pending {
        g(cx).focus_pending = false;
        if let Some(f) = field.clone() { window.defer(cx, move |window, cx| f.update(cx, |s, cx| s.focus(window, cx))); }
    }
    let vs = window.viewport_size();
    let narrow = f32::from(vs.width) <= NARROW;
    let m = if narrow { T::SP_12 } else { T::SP_16 };
    let width = if narrow { vs.width - m * 2. } else { T::DIALOG_W_LG.min(vs.width - m * 2.) };
    let r = cx.global::<Sreq>();
    let armed = r.armed.clone();
    let list = r.list.clone();
    let mut col = div().id("sreq-dock").relative().occlude().w(width).max_h(vs.height - T::TOPBAR_H - T::SP_16 - m).overflow_y_scroll()
        .flex().flex_col().gap(T::SP_10)
        // A press on the dock is the dock's (the pane beneath keeps its focus).
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    for (i, q) in list.iter().enumerate() {
        col = col.child(card(i, q, armed.as_deref() == Some(q.id.as_str()), field.clone(), cx));
    }
    col = col.child(canvas(|bounds, _, cx| { g(cx).bounds = Some(bounds); }, |_, _, _, _| {}).absolute().size_full());
    Some(deferred(anchored().anchor(Anchor::BottomRight).position(point(vs.width - m, vs.height - m)).child(col)).with_priority(1).into_any_element())
}

/// The driver's actions are this part's own (`init`).
pub fn test_ui(_c: &mut ComposerView, _name: &str, _args: &[String], _window: &mut Window, _cx: &mut Context<ComposerView>) -> Option<Value> { None }

/// No pane fields: the dock is the window's.
pub fn pane_props(_c: &ComposerView, _cx: &App) -> Value { json!({}) }

/// The window-wide props are this part's own (`init`).
pub fn props(_c: &ComposerView, _cx: &App) -> Value { json!({}) }

// ─── The part ───

fn by_name(name: &str, cx: &App) -> String {
    cx.try_global::<Sreq>().and_then(|r| r.list.iter().find(|q| q.name == name).map(|q| q.id.clone())).unwrap_or_default()
}

fn test_action(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    match name {
        // Approve on a card: the armed one approves, another takes the field.
        "sreq-approve" => {
            let id = by_name(&arg(0), cx);
            if g(cx).armed.as_deref() == Some(id.as_str()) { confirm(window, cx); } else { arm_by_click(&id, cx); }
        }
        "sreq-reject" => { let id = by_name(&arg(0), cx); if !id.is_empty() { reject(id, cx); } }
        "sreq-allow" => { let r = g(cx); r.allow_on = !r.allow_on; cx.refresh_windows(); }
        "sreq-duration" => { g(cx).duration = arg(0); cx.refresh_windows(); }
        // The armed field takes the focus (as a click on it), its text selected.
        "sreq-focus" => focus_field(window, cx),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

fn part_props(window: &mut Window, cx: &mut App) -> Value {
    let r = g(cx);
    if r.list.is_empty() { r.bounds = None; }
    let r = cx.global::<Sreq>();
    let focused = r.input.as_ref().is_some_and(|(i, _, _)| i.focus_handle(cx).is_focused(window));
    let dock = r.bounds.map(|b| json!({ "x": f32::from(b.origin.x), "y": f32::from(b.origin.y), "w": f32::from(b.size.width), "h": f32::from(b.size.height) }))
        .unwrap_or_else(|| json!({ "x": 0, "y": 0, "w": 0, "h": 0 }));
    let sreq = if r.list.is_empty() { Value::Null } else {
        json!({
            "cards": r.list.iter().map(|q| json!({ "id": q.id, "name": q.name, "run": q.run, "command": q.command, "armed": r.armed.as_deref() == Some(q.id.as_str()) })).collect::<Vec<_>>(),
            "armed": r.armed, "err": r.err, "busy": r.busy, "allowOn": r.allow_on, "duration": r.duration,
            // How much is typed (never what).
            "typed": typed(cx).chars().count(),
        })
    };
    json!({ "sreq": sreq, "sreqFocused": focused, "dock": dock })
}

/// Register the part (lib.rs's init calls this once).
pub fn init(cx: &mut App) {
    if !cx.has_global::<Sreq>() { cx.set_global(Sreq::default()); }
    cx.bind_keys([KeyBinding::new("escape", SreqEsc, Some("SreqField"))]);
    crundi_shell::register(cx, Area::new("sreq")
        .on_event(on_event)
        .on_signed_in(load)
        .on_signed_out(|cx| { arm(None, false, cx); g(cx).list.clear(); })
        .test_ui(test_action)
        .props(part_props));
}
