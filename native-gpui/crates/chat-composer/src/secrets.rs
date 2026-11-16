//! 2.12: the chat secrets dialog: list, add, remove, the PIN step.
//! Spec: native-client/specs/chat-secrets.md; the Slint client's app.rs
//! (sec_*), ui/secrets.slint; the web's app/vendor/crundi-chat-secrets.js.
//!
//! What the pane header's lock opens, for that chat: the secrets
//! pre-authorised for it (name, how long), Revoke / Revoke all, and
//! pre-authorising more (tick secrets, type each one's PIN or passphrase,
//! a duration, Pre-authorise: all or nothing). One dialog for the window,
//! modal, through the widgets crate's `overlay_modal` (the scrim over the
//! window, drawn by the chat's composer). Its state is one Global (`Sec`), with the
//! server's capability and every chat's grants (loaded at sign-in, kept live
//! from the `chat-secrets` event), which the lock's count reads.
//!
//! This file registers its own part ("chat-secrets": the live events, the
//! driver's sec-* actions, the props secretsDialog / secProbe / pointerModal
//! / toast) from `init`, which lib.rs calls. The lock itself sits in the pane
//! header (Workspace's): `lock_button` is ready for it (shared/requests.md).

use crate::ComposerView;
use crundi_core::{api::Api, head, secrets as rules, time};
use crundi_shell::{Area, AreaEvent};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

actions!(chat_secrets, [SecNext, SecPrev]);

const UNSUPPORTED: &str = "This server does not support this yet. Update the server to use it.";
const DURATIONS: [(&str, &str); 4] = [("15m", "15 minutes"), ("1h", "1 hour"), ("8h", "8 hours"), ("chat", "Until this chat ends")];
const HINT: &str = "A pre-authorised secret lets this chat run commands with it without asking for the PIN or passphrase. Only to run: its value is never shown, and Claude reading a value still asks you every time. They are kept in the server\u{2019}s memory only, for this chat only.";

/// This feature's state inside one chat's composer (none: the dialog is the window's).
#[derive(Default)]
pub struct State {}

/// A ticked secret's lock field.
struct Field {
    input: Entity<w::InputState>,
    _sub: Subscription,
}

/// The open dialog: for which chat, the secrets to pick (id, name, lock;
/// None while loading), what is ticked and typed (dropped on close).
struct Dlg {
    serial: u64,
    chat: String,
    secrets: Option<Vec<(String, String, String)>>,
    chosen: Vec<String>,
    fields: HashMap<String, Field>,
    bad: String,
    duration: String,
    busy: bool,
    err: String,
    revoking: HashSet<String>,
    unsupported: bool,
    focus: FocusHandle,
    scroll: ScrollHandle,
}

/// The window's secrets state: the capability, every chat's grants, the dialog.
#[derive(Default)]
pub struct Sec {
    /// The server can pre-authorise ("chat-secret-preauth").
    pub cap: bool,
    /// Each chat's grants (the server's list), by chat id.
    pub grants: HashMap<String, Value>,
    dlg: Option<Dlg>,
    serial: u64,
    /// The last toast the composer's parts showed (the driver's `toast`).
    pub last_toast: String,
    /// The foot's place on the last frame (the driver's secProbe).
    foot: Option<Bounds<Pixels>>,
}
impl Global for Sec {}

fn sec(cx: &mut App) -> &mut Sec {
    if !cx.has_global::<Sec>() { cx.set_global(Sec::default()); }
    cx.global_mut::<Sec>()
}

/// The server can pre-authorise secrets for a chat.
pub fn cap(cx: &App) -> bool { cx.try_global::<Sec>().is_some_and(|s| s.cap) }

/// How many grants a chat holds now (expired ones not counted).
pub fn grant_n(chat: &str, cx: &App) -> usize {
    head::grant_count(cx.try_global::<Sec>().and_then(|s| s.grants.get(chat)), time::now_secs())
}

/// A chat is a collaborator's (no lock, no "Allow for this chat").
pub(crate) fn collaborator(chat: &str, cx: &App) -> bool {
    crundi_shell::session::get(cx).terminals.iter()
        .find(|t| t.get("id").and_then(|i| i.as_str()) == Some(chat))
        .and_then(|c| c.get("collaborator"))
        .is_some_and(|v| v.as_bool().unwrap_or(!v.is_null()))
}

/// Run with the window (the answers of server calls come with the App only).
pub(crate) fn with_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App)) {
    if let Some(h) = cx.active_window().or_else(|| cx.windows().first().copied()) {
        let _ = h.update(cx, |_, window, cx| f(window, cx));
    }
}

/// The toast (top right), remembered for the driver.
pub(crate) fn toast(cx: &mut App, text: &str, kind: w::Toast) {
    sec(cx).last_toast = text.to_string();
    let t = text.to_string();
    with_window(cx, move |window, cx| w::toast(window, cx, t, kind));
}

/// A POST's status and body as they came (a refusal's body names the secret).
pub(crate) fn post_raw(api: &Api, path: &str, body: &Value) -> Result<(u16, Value), String> {
    let bytes = serde_json::to_vec(body).unwrap_or_default();
    let (status, text) = api.post_with_progress(path, &bytes, &|_, _| {}).map_err(|e| e.to_string())?;
    if status == 0 { return Err("Network error".into()); }
    Ok((status, serde_json::from_str(&text).unwrap_or(Value::Null)))
}

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

fn refresh(cx: &mut App) { cx.refresh_windows(); }

// ─── Loading and live ───

/// After sign-in: the capability (the handshake), then every chat's grants.
fn load(cx: &mut App) {
    let Some(api) = crundi_shell::api(cx) else { return };
    let base = api.base().to_string();
    crundi_shell::run(cx, move || crundi_core::api::handshake(&base).ok().and_then(|h| h.pointer("/server/capabilities").cloned()), |caps, cx| {
        let on = caps.as_ref().and_then(|c| c.as_array().map(|c| c.iter().any(|x| x == "chat-secret-preauth"))).unwrap_or(false);
        sec(cx).cap = on;
        refresh(cx);
        if !on { return; }
        crundi_shell::call(cx, |api| api.get("/api/secrets/chat"), |r, cx| {
            if let Some(chats) = r.ok().and_then(|v| v.get("chats").and_then(|c| c.as_object()).cloned()) {
                let g = &mut sec(cx).grants;
                for (sid, list) in chats { g.insert(sid, list); }
            }
            refresh(cx);
        });
    });
}

fn on_event(e: &AreaEvent, cx: &mut App) {
    if let AreaEvent::Server("chat-secrets", v) = e {
        let sid = s(v, "sessionId");
        if sid.is_empty() { return; }
        // A grant on its way for this chat: its answer brings the list (the
        // event can come first, and the dialog changes at once with the answer).
        if cx.try_global::<Sec>().and_then(|s| s.dlg.as_ref()).is_some_and(|d| d.busy && d.chat == sid) { return; }
        sec(cx).grants.insert(sid, v.get("grants").cloned().unwrap_or(Value::Null));
        refresh(cx);
    }
}

// ─── Opening and closing ───

/// The lock: the dialog for this chat (one at a time; opening it again
/// replaces it). The grants known now at once; then the chat's own and the
/// secrets to pick, from the server.
pub fn open(chat: &str, window: &mut Window, cx: &mut App) {
    if sec(cx).dlg.is_some() { close(window, cx); }
    let cap = cap(cx);
    let focus = cx.focus_handle();
    let st = sec(cx);
    st.serial += 1;
    let serial = st.serial;
    st.dlg = Some(Dlg {
        serial, chat: chat.to_string(), secrets: None, chosen: vec![], fields: HashMap::new(), bad: String::new(), duration: "1h".into(),
        busy: false, err: if cap { String::new() } else { UNSUPPORTED.into() }, revoking: HashSet::new(), unsupported: !cap,
        focus: focus.clone(), scroll: ScrollHandle::new(),
    });
    window.focus(&focus, cx);
    refresh(cx);
    if !cap { return; }
    let id = chat.to_string();
    crundi_shell::call(cx, move |api| api.get(&format!("/api/secrets/chat?sessionId={id}")), move |r, cx| {
        let st = sec(cx);
        let mine = st.dlg.as_ref().is_some_and(|d| d.serial == serial);
        match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => { if mine { if let Some(d) = st.dlg.as_mut() { d.err = s(&v, "error"); } } }
            Ok(v) => { if let Some(d) = st.dlg.as_ref().filter(|d| d.serial == serial) { let c = d.chat.clone(); st.grants.insert(c, v.get("grants").cloned().unwrap_or(Value::Null)); } }
            Err(e) => { if mine && e != "Not supported by this server" { if let Some(d) = st.dlg.as_mut() { d.err = e; } } }
        }
        refresh(cx);
    });
    crundi_shell::call(cx, |api| api.get("/api/secrets"), move |r, cx| {
        if let Some(d) = sec(cx).dlg.as_mut().filter(|d| d.serial == serial) {
            d.secrets = Some(r.ok().and_then(|v| v.get("secrets").and_then(|x| x.as_array()).cloned()).unwrap_or_default().iter()
                .map(|x| (s(x, "id"), s(x, "name"), { let l = s(x, "lock"); if l.is_empty() { "passphrase".into() } else { l } }))
                .collect());
        }
        refresh(cx);
    });
}

/// Close (×, Close, the driver): the layer and the state.
pub fn close(window: &mut Window, cx: &mut App) {
    let Some(serial) = cx.try_global::<Sec>().and_then(|s| s.dlg.as_ref().map(|d| d.serial)) else { return };
    forget(serial, window, cx);
}

/// The dialog's state dropped, everything typed overwritten first.
fn forget(serial: u64, window: &mut Window, cx: &mut App) {
    let st = sec(cx);
    if !st.dlg.as_ref().is_some_and(|d| d.serial == serial) { return; }
    let d = st.dlg.take();
    st.foot = None;
    if let Some(d) = d {
        for f in d.fields.values() { f.input.update(cx, |s, cx| s.set_value("", window, cx)); }
    }
    refresh(cx);
}

// ─── Choosing ───

fn lock_of(d: &Dlg, id: &str) -> String {
    d.secrets.iter().flatten().find(|x| x.0 == id).map(|x| x.2.clone()).unwrap_or_default()
}

/// Tick or untick a secret: its field appears and takes the focus, or goes
/// with what was typed in it.
fn tick(id: &str, window: &mut Window, cx: &mut App) {
    let Some(d) = sec(cx).dlg.as_mut() else { return };
    if let Some(i) = d.chosen.iter().position(|x| x == id) {
        d.chosen.remove(i);
        if d.bad == id { d.bad.clear(); }
        let f = d.fields.remove(id);
        let focus = d.focus.clone();
        if let Some(f) = f { f.input.update(cx, |s, cx| s.set_value("", window, cx)); }
        window.focus(&focus, cx);
    } else {
        d.chosen.push(id.to_string());
        let lock = lock_of(d, id);
        let input = cx.new(|cx| w::InputState::new(window, cx).masked(true).placeholder(if lock == "pin" { "PIN" } else { "Passphrase" }));
        let sid = id.to_string();
        let sub = window.subscribe(&input, cx, move |input, ev: &w::InputEvent, window, cx| match ev {
            w::InputEvent::PressEnter { .. } => grant(window, cx),
            w::InputEvent::Change => {
                // A PIN takes digits only, at most 6.
                let lock = cx.try_global::<Sec>().and_then(|s| s.dlg.as_ref()).map(|d| lock_of(d, &sid)).unwrap_or_default();
                let text = input.read(cx).value().to_string();
                let kept = rules::typed(&lock, &text);
                if kept != text { input.update(cx, |s, cx| s.set_value(kept, window, cx)); }
                refresh(cx);
            }
            _ => {}
        });
        input.update(cx, |s, cx| s.focus(window, cx));
        if let Some(d) = sec(cx).dlg.as_mut() { d.fields.insert(id.to_string(), Field { input, _sub: sub }); }
    }
    refresh(cx);
}

/// Tab and Shift+Tab among the ticked secrets' fields (the mode picker's
/// rule: the keyboard stays in the dialog). Elsewhere, Tab as usual.
fn next_field(back: bool, window: &mut Window, cx: &mut App) {
    let Some(d) = cx.try_global::<Sec>().and_then(|s| s.dlg.as_ref()) else { cx.propagate(); return };
    let order: Vec<Entity<w::InputState>> = d.secrets.iter().flatten().filter(|x| d.chosen.contains(&x.0)).filter_map(|x| d.fields.get(&x.0).map(|f| f.input.clone())).collect();
    let Some(i) = order.iter().position(|f| f.focus_handle(cx).is_focused(window)) else { cx.propagate(); return };
    let n = order.len();
    let next = order[if back { (i + n - 1) % n } else { (i + 1) % n }].clone();
    next.update(cx, |s, cx| s.focus(window, cx));
    refresh(cx);
}

fn set_duration(dur: &str, cx: &mut App) {
    if let Some(d) = sec(cx).dlg.as_mut() { d.duration = dur.to_string(); }
    refresh(cx);
}

// ─── Pre-authorise ───

/// The two checks here, then the server (all or nothing).
fn grant(window: &mut Window, cx: &mut App) {
    let body = {
        let typed: HashMap<String, String> = cx.try_global::<Sec>().and_then(|s| s.dlg.as_ref())
            .map(|d| d.fields.iter().map(|(k, f)| (k.clone(), f.input.read(cx).value().to_string())).collect()).unwrap_or_default();
        let Some(d) = sec(cx).dlg.as_mut() else { return };
        if d.busy || d.unsupported { return; }
        if d.chosen.is_empty() { d.err = "Choose at least one secret.".into(); refresh(cx); return; }
        let empty = String::new();
        if let Some(bad) = d.chosen.iter().find(|id| !rules::lock_ok(&lock_of(d, id), typed.get(*id).unwrap_or(&empty))).cloned() {
            d.bad = bad.clone();
            d.err = "Enter the PIN or passphrase of each secret chosen.".into();
            let f = d.fields.get(&bad).map(|f| f.input.clone());
            if let Some(f) = f { f.update(cx, |s, cx| s.focus(window, cx)); }
            refresh(cx);
            return;
        }
        d.bad.clear();
        d.err.clear();
        d.busy = true;
        json!({ "action": "grant", "sessionId": d.chat, "duration": d.duration,
            "secrets": d.chosen.iter().map(|id| json!({ "id": id, "pin": typed.get(id).cloned().unwrap_or_default() })).collect::<Vec<_>>() })
    };
    refresh(cx);
    let Some(api) = crundi_shell::api(cx) else { return };
    let chat = s(&body, "sessionId");
    let serial = cx.try_global::<Sec>().and_then(|s| s.dlg.as_ref()).map(|d| d.serial).unwrap_or(0);
    crundi_shell::run(cx, move || {
        let mut body = body;
        let r = post_raw(&api, "/api/secrets/chat", &body);
        // The locks in the body: overwritten once sent.
        if let Some(list) = body.get_mut("secrets").and_then(|l| l.as_array_mut()) { for x in list { x["pin"] = json!(""); } }
        r
    }, move |r, cx| {
        let st = sec(cx);
        if let Ok((_, v)) = &r { if let Some(g) = v.get("grants") { st.grants.insert(chat.clone(), g.clone()); } }
        let Some(d) = st.dlg.as_mut().filter(|d| d.serial == serial) else { refresh(cx); return };
        d.busy = false;
        let mut ok = false;
        let mut empty_field: Option<Entity<w::InputState>> = None;
        let mut gone: Vec<Field> = vec![];
        match r {
            Ok((st, v)) if st < 300 && v.get("ok").and_then(|o| o.as_bool()) == Some(true) => {
                gone = d.fields.drain().map(|(_, f)| f).collect();
                d.chosen.clear();
                d.bad.clear();
                ok = true;
            }
            Ok((404, _)) => d.err = UNSUPPORTED.into(),
            Ok((_, v)) => {
                let e = s(&v, "error");
                let sid = s(&v, "secretId");
                match d.secrets.iter().flatten().find(|x| x.0 == sid && !sid.is_empty()).map(|x| x.1.clone()) {
                    Some(name) => {
                        empty_field = d.fields.get(&sid).map(|f| f.input.clone());
                        d.bad = sid;
                        d.err = format!("{name}: {e}");
                    }
                    None => d.err = if e.is_empty() { "Nothing could be pre-authorised for this chat.".into() } else { e },
                }
            }
            Err(e) => d.err = e,
        }
        let focus = d.focus.clone();
        with_window(cx, move |window, cx| {
            for f in &gone { f.input.update(cx, |s, cx| s.set_value("", window, cx)); }
            if let Some(f) = &empty_field { f.update(cx, |s, cx| { s.set_value("", window, cx); s.focus(window, cx); }); }
            if !gone.is_empty() { window.focus(&focus, cx); }
            drop(gone);
        });
        refresh(cx);
        if ok { toast(cx, "Pre-authorised for this chat", w::Toast::Info); }
    });
}

/// Revoke one (Some) or all (None): no confirmation; the button pending.
fn revoke(one: Option<String>, cx: &mut App) {
    let job = {
        let st = sec(cx);
        let Some(d) = st.dlg.as_mut() else { return };
        let tag = one.clone().unwrap_or_else(|| "*all*".into());
        if !d.revoking.insert(tag.clone()) { return; }
        let name = one.as_ref().and_then(|id| st.grants.get(&d.chat).and_then(|g| g.as_array()).and_then(|g| g.iter().find(|x| s(x, "secretId") == *id)).map(|x| s(x, "name"))).unwrap_or_default();
        let body = match &one { Some(id) => json!({ "action": "revoke", "sessionId": d.chat, "secretId": id }), None => json!({ "action": "revokeAll", "sessionId": d.chat }) };
        (d.chat.clone(), tag, name, body)
    };
    refresh(cx);
    let (chat, tag, name, body) = job;
    let Some(api) = crundi_shell::api(cx) else { return };
    let all = one.is_none();
    crundi_shell::run(cx, move || post_raw(&api, "/api/secrets/chat", &body), move |r, cx| {
        let st = sec(cx);
        if let Some(d) = st.dlg.as_mut() { d.revoking.remove(&tag); }
        let (msg, err) = match r {
            Ok((code, v)) if code < 300 && v.get("ok").and_then(|o| o.as_bool()) == Some(true) => {
                if let Some(g) = v.get("grants") { st.grants.insert(chat, g.clone()); }
                (if all { "Revoked all for this chat".to_string() } else { format!("Revoked {name}") }, false)
            }
            Ok((404, _)) => (UNSUPPORTED.to_string(), true),
            Ok((_, v)) => { let e = s(&v, "error"); (if !e.is_empty() { e } else if all { "Could not revoke them".into() } else { "Could not revoke it".into() }, true) }
            Err(e) => (e, true),
        };
        refresh(cx);
        toast(cx, &msg, if err { w::Toast::Error } else { w::Toast::Info });
    });
}

// ─── Drawing ───

/// The grants of a chat that have not run out, as (id, name, when).
fn grants_of(chat: &str, cx: &App) -> Vec<(String, String, String)> {
    let now = chrono::Local::now();
    let now_s = time::now_secs();
    cx.try_global::<Sec>().and_then(|s| s.grants.get(chat)).and_then(|g| g.as_array()).cloned().unwrap_or_default().iter()
        .filter(|g| match g.get("expiresAt").and_then(|e| e.as_str()) { Some(at) if !at.is_empty() => time::parse_iso(at).map(|t| t > now_s).unwrap_or(true), _ => true })
        .map(|g| (s(g, "secretId"), s(g, "name"), rules::when(g.get("expiresAt").and_then(|e| e.as_str()), now)))
        .collect()
}

fn label(text: &'static str) -> Div {
    div().pt(T::SP_4).text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(text.to_uppercase())
}

fn line(text: impl Into<SharedString>) -> Div {
    div().text_size(T::TEXT_13).text_color(T::text_muted()).child(text.into())
}

/// The dialog's body: head, hint, the grants, the pick list, the durations.
fn body(window: &mut Window, cx: &mut App) -> AnyElement {
    let Some(d) = cx.try_global::<Sec>().and_then(|s| s.dlg.as_ref()) else { return div().into_any_element() };
    let grants = grants_of(&d.chat, cx);
    let revoking = d.revoking.clone();
    let focus = d.focus.clone();
    let picks: Vec<(String, String, bool, bool, Option<Entity<w::InputState>>)> = d.secrets.iter().flatten()
        .map(|(id, name, _)| (id.clone(), name.clone(), d.chosen.contains(id), d.bad == *id, d.fields.get(id).map(|f| f.input.clone()))).collect();
    let pick_line = match &d.secrets { None => Some("Loading the secrets…"), Some(v) if v.is_empty() => Some("There are no secrets yet. Add them under Secrets."), _ => None };
    let duration = d.duration.clone();
    let _ = window;

    let mut list = div().flex().flex_col().gap(T::SP_6);
    if grants.is_empty() { list = list.child(line("Nothing is pre-authorised for this chat.")); }
    for (i, (id, name, when)) in grants.iter().cloned().enumerate() {
        let busy = revoking.contains(&id);
        list = list.child(div().flex().items_center().gap(T::SP_10).py(T::SP_8).px(T::SP_10).rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg())
            .child(div().flex().flex_col().flex_1().min_w_0()
                .child(div().font_family(T::MONO).text_size(T::TEXT_13).text_color(T::text()).child(name))
                .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(when)))
            .child(w::disabled(w::button(("sec-revoke", i), "Revoke"), busy).on_click(move |_, _, cx| revoke(Some(id.clone()), cx))));
    }
    if grants.len() >= 2 {
        let busy = revoking.contains("*all*");
        list = list.child(div().flex().justify_end().child(w::disabled(w::button("sec-revoke-all", "Revoke all"), busy).text_color(T::red()).on_click(|_, _, cx| revoke(None, cx))));
    }

    let mut pick = div().id("sec-pick").flex().flex_col().gap(T::SP_6).max_h(T::SEC_PICK_MAX_H).overflow_y_scroll()
        .key_context("SecLocks")
        .on_action(|_: &SecNext, window, cx| next_field(false, window, cx))
        .on_action(|_: &SecPrev, window, cx| next_field(true, window, cx));
    if let Some(l) = pick_line { pick = pick.child(line(l)); }
    for (i, (id, name, chosen, bad, field)) in picks.into_iter().enumerate() {
        pick = pick.child(div().flex().items_center().gap(T::SP_8).py(T::SP_4).px(T::SP_2)
            .child(div().flex_1().min_w_0().font_family(T::MONO).text_size(T::TEXT_13)
                .child(w::check(("sec-tick", i), name, chosen, move |_, window, cx| tick(&id, window, cx))))
            .when_some(field.filter(|_| chosen), |r, f| r.child(div().flex_none().w(T::SEC_LOCK_FIELD_W)
                .on_key_down(type_shim(f.clone())).child(w::input(&f).when(bad, |i| i.border_color(T::red()))))));
    }

    let mut chips = div().flex().flex_wrap().gap(T::SP_6);
    for (i, (key, text)) in DURATIONS.iter().enumerate() {
        chips = chips.child(w::toggle(("sec-dur", i), *text, duration == *key).on_click(move |_, _, cx| set_duration(key, cx)));
    }

    let _ = focus;
    div().id("sec-body").flex().flex_col().flex_shrink(1.).min_h_0().overflow_y_scroll().gap(T::SP_10).p(T::SP_10)
        .child(div().flex().items_center().gap(T::SP_8)
            .child(div().text_size(T::ICON_LG).text_color(T::accent_hover()).child(IconName::Lock))
            .child(div().flex_1().text_size(T::TEXT_15).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Secrets for this chat"))
            .child(w::icon_button("sec-x", IconName::X, "Close").on_click(|_, window, cx| close(window, cx))))
        .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).line_height(relative(T::LH_CHAT)).child(HINT))
        .child(label("Pre-authorised now"))
        .child(list)
        .child(label("Pre-authorise"))
        .child(pick)
        .child(chips)
        .into_any_element()
}

/// The foot (always in view): the error line, Close and Pre-authorise.
fn foot(_window: &mut Window, cx: &mut App) -> AnyElement {
    let Some(d) = cx.try_global::<Sec>().and_then(|s| s.dlg.as_ref()) else { return div().into_any_element() };
    let (err, busy, unsupported) = (d.err.clone(), d.busy, d.unsupported);
    div().relative().flex_none().flex().flex_col().gap(T::SP_8).px(T::SP_10).pb(T::SP_10)
        .when(!err.is_empty(), |f| f.child(div().text_size(T::TEXT_12).text_color(T::red()).child(err)))
        .child(div().flex().justify_end().gap(T::SP_8)
            .child(w::button("sec-close", "Close").on_click(|_, window, cx| close(window, cx)))
            .child(w::disabled(w::primary("sec-grant", if busy { "Checking…" } else { "Pre-authorise" }), busy || unsupported).on_click(|_, window, cx| grant(window, cx))))
        .child(canvas(|bounds, _, cx| { sec(cx).foot = Some(bounds); }, |_, _, _, _| {}).absolute().size_full())
        .into_any_element()
}

/// The pane header's lock, for Workspace to place (shared/requests.md): the
/// lock and the count, yellow when some are pre-authorised, the tooltip;
/// None without the capability or on a collaborator's chat.
pub fn lock_button(chat: &str, cx: &App) -> Option<AnyElement> {
    if !cap(cx) || collaborator(chat, cx) { return None; }
    let n = grant_n(chat, cx);
    let id = chat.to_string();
    let b = w::icon_button(SharedString::from(format!("sec-lock-{chat}")), IconName::Lock, head::secrets_tip(n))
        .when(n > 0, |b| b.label(n.to_string()).text_color(T::yellow()))
        .on_click(move |_, window, cx| open(&id, window, cx));
    Some(b.into_any_element())
}

// ─── The composer's hooks (lib.rs) ───

/// Nothing in the composer itself: the dialog is the window's.
pub fn render(c: &mut ComposerView, window: &mut Window, cx: &mut Context<ComposerView>) -> Option<AnyElement> {
    let (chat, focus, scroll, serial) = cx.try_global::<Sec>()?.dlg.as_ref().map(|d| (d.chat.clone(), d.focus.clone(), d.scroll.clone(), d.serial))?;
    let me = c.chat_id(cx);
    // The dialog's chat's composer draws it (else the dock's host).
    let host = if crate::composer(cx, &chat).is_some() { chat == me } else { crate::sreq::is_host(&me, cx) };
    if !host { return None; }
    let (b, f) = (body(window, cx), foot(window, cx));
    let m = w::Modal {
        id: "sec-modal", width: T::DIALOG_W_LG, focus, scroll,
        on_close: std::rc::Rc::new(move |window, cx| forget(serial, window, cx)),
        on_key: std::rc::Rc::new(|e, window, cx| if e.keystroke.key == "escape" { close(window, cx); cx.stop_propagation(); }),
    };
    Some(w::overlay_modal(m, window, vec![b, f]).into_any_element())
}

/// The test driver's typed keys: under the driver only, typed text goes into
/// a focused field as the platform's input handler would (the driver's key
/// events don't reach it yet; shared/requests.md asks the Lead's driver).
pub(crate) fn type_shim(input: Entity<w::InputState>) -> impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static {
    move |e, window, cx| if std::env::var_os("CRUNDI_TEST_PORT").is_some() {
        let k = &e.keystroke;
        if let (Some(ch), true) = (k.key_char.clone(), k.modifiers.is_subset_of(&Modifiers::shift())) {
            input.update(cx, |st, cx| st.replace(ch, window, cx));
            cx.stop_propagation();
        }
    }
}

/// The driver's actions are this part's own (`init`), not the composer's.
pub fn test_ui(_c: &mut ComposerView, _name: &str, _args: &[String], _window: &mut Window, _cx: &mut Context<ComposerView>) -> Option<Value> { None }

/// The lock's fields of the chat's pane (the Slint client's names).
pub fn pane_props(c: &ComposerView, cx: &App) -> Value {
    let id = c.chat_id(cx);
    let n = grant_n(&id, cx);
    json!({ "secrets": cap(cx) && !collaborator(&id, cx), "secretsN": n, "secretsTip": head::secrets_tip(n) })
}

/// The window-wide props are this part's own (`init`).
pub fn props(_c: &ComposerView, cx: &App) -> Value {
    // The composer ORs every feature's pointerModal.
    json!({ "pointerModal": cx.try_global::<Sec>().is_some_and(|s| s.dlg.is_some()) })
}

// ─── The part ───

/// The chat of a pane, by its leaf id (the driver's sec-open).
fn chat_of_leaf(leaf: &str, cx: &mut App) -> Option<String> {
    let panes = crundi_workbench::workbench(cx).map(|w| w.update(cx, |w, cx| w.panes(cx))).unwrap_or_default();
    panes.iter().find(|p| p.get("id").map(|i| i.to_string()) == Some(leaf.to_string()))
        .and_then(|p| p.get("key").and_then(|k| k.as_str()).and_then(|k| k.strip_prefix("live:")).map(String::from))
}

fn id_by_name(name: &str, cx: &App) -> String {
    let Some(st) = cx.try_global::<Sec>() else { return String::new() };
    let Some(d) = st.dlg.as_ref() else { return String::new() };
    d.secrets.iter().flatten().find(|x| x.1 == name).map(|x| x.0.clone())
        .or_else(|| grants_of(&d.chat, cx).into_iter().find(|g| g.1 == name).map(|g| g.0))
        .unwrap_or_default()
}

fn test_action(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    match name {
        "sec-open" => match chat_of_leaf(&arg(0), cx) {
            Some(chat) => open(&chat, window, cx),
            None => return Some(json!({ "ok": false, "error": "no such chat pane" })),
        },
        "sec-tick" => { let id = id_by_name(&arg(0), cx); if !id.is_empty() { tick(&id, window, cx); } }
        "sec-type" => {
            let id = id_by_name(&arg(0), cx);
            let f = cx.try_global::<Sec>().and_then(|s| s.dlg.as_ref()).and_then(|d| d.fields.get(&id)).map(|f| f.input.clone());
            let t = arg(1);
            if let Some(f) = f { f.update(cx, |s, cx| s.set_value(t, window, cx)); }
        }
        "sec-duration" => set_duration(&arg(0), cx),
        "sec-grant" => grant(window, cx),
        "sec-revoke" => { let id = id_by_name(&arg(0), cx); if !id.is_empty() { revoke(Some(id), cx); } }
        "sec-revoke-all" => revoke(None, cx),
        "sec-close" => close(window, cx),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

fn part_props(window: &mut Window, cx: &mut App) -> Value {
    let vh = window.viewport_size().height;
    sec(cx);
    let st = cx.global::<Sec>();
    let dialog = match st.dlg.as_ref() {
        None => Value::Null,
        Some(d) => {
            let typed: serde_json::Map<String, Value> = d.secrets.iter().flatten()
                .filter_map(|x| d.fields.get(&x.0).map(|f| (x.1.clone(), json!(f.input.read(cx).value().chars().count())))).collect();
            json!({
                "chat": d.chat, "loading": d.secrets.is_none(), "secrets": d.secrets.iter().flatten().map(|x| x.1.clone()).collect::<Vec<_>>(),
                "chosen": d.chosen, "bad": d.bad, "typed": typed, "duration": d.duration, "busy": d.busy, "err": d.err, "unsupported": d.unsupported,
                "grants": grants_of(&d.chat, cx).into_iter().map(|g| json!({ "name": g.1, "when": g.2 })).collect::<Vec<_>>(),
            })
        }
    };
    let st = sec(cx);
    let probe = match (st.dlg.is_some(), st.foot) {
        (true, Some(b)) => json!({ "actionsShown": b.size.height > px(0.) && b.origin.y >= px(0.) && b.origin.y + b.size.height <= vh }),
        (true, None) => json!({ "actionsShown": false }),
        _ => Value::Null,
    };
    json!({ "secretsDialog": dialog, "secProbe": probe, "toast": st.last_toast })
}

/// Register the part (lib.rs's init calls this once): the live events, the
/// driver's sec-* actions and the props; Tab among the lock fields; the
/// "when" lines and expiries every 30 s.
pub fn init(cx: &mut App) {
    if !cx.has_global::<Sec>() { cx.set_global(Sec::default()); }
    cx.bind_keys([KeyBinding::new("tab", SecNext, Some("SecLocks")), KeyBinding::new("shift-tab", SecPrev, Some("SecLocks"))]);
    crundi_shell::register(cx, Area::new("chat-secrets")
        .on_event(on_event)
        .on_signed_in(load)
        .on_signed_out(|cx| {
            with_window(cx, |window, cx| close(window, cx));
            let st = sec(cx);
            st.cap = false;
            st.grants.clear();
        })
        .test_ui(test_action)
        .props(part_props));
    cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor().timer(std::time::Duration::from_secs(30)).await;
        cx.update(|cx| cx.refresh_windows());
    }).detach();
}
