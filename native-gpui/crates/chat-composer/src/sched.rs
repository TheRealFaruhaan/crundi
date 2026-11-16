//! 2.10: scheduled messages (specs/scheduled-messages.md; the Slint
//! client's SchedPanel and src/app.rs "Scheduled messages"): the clock in
//! the meta row toggles the panel (.cc-schedpanel) over the chat's foot, not
//! modal: a head ("Schedule a message", ×), the message (the composer's text
//! carried in), the triggers (At a time, Turn ends, Limit resets: only with
//! the limit warmer), the time field (now + 30 minutes), the note, Schedule,
//! then this chat's pending messages (the first is "next"), each with ✕.
//! The rules are crundi_core::sched's; GET/POST /api/chat-schedule,
//! DELETE /api/chat-schedule/:id.

use crate::ComposerView;
use crundi_core::api::{ApiError, ApiResult};
use crundi_core::sched as rules;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};

#[derive(Clone)]
pub struct Item { pub id: String, pub when: String, pub text: String, pub next: bool }

/// The panel of one chat: open, the form, the list.
#[derive(Default)]
pub struct State {
    pub open: bool,
    text: Option<Entity<w::TextareaState>>,
    at: Option<Entity<w::InputState>>,
    pub trigger: String,
    pub note: String,
    pub items: Vec<Item>,
    /// The limit warmer is on; None: not known yet.
    pub warm: Option<bool>,
    /// The list's own line: "" (items), none, an error, unsupported.
    pub list: String,
    pub unsupported: bool,
    /// The list has answered (nothing shows until it does, as the web).
    pub loaded: bool,
}

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

fn enc(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// A test stub as a server answer (the driver's "stub"): a status of 400 or
/// more is that error, "net" a network failure, else the answer itself.
pub(crate) fn stub_result(v: Value) -> ApiResult {
    if let Some(ms) = v.get("delayMs").and_then(|x| x.as_u64()) { std::thread::sleep(std::time::Duration::from_millis(ms)); }
    match v.get("status").and_then(|x| x.as_u64()) {
        Some(st) if st >= 400 => Err(ApiError::Status(st as u16, s(&v, "error"))),
        _ if v.get("net").is_some() => Err(ApiError::Net(s(&v, "net"))),
        _ => Ok(v),
    }
}

fn text_of(c: &ComposerView, cx: &App) -> String { c.sched.text.as_ref().map(|t| t.read(cx).value().to_string()).unwrap_or_default() }
fn at_of(c: &ComposerView, cx: &App) -> String { c.sched.at.as_ref().map(|t| t.read(cx).value().to_string()).unwrap_or_default() }

fn set_text(c: &mut ComposerView, v: &str, window: &mut Window, cx: &mut Context<ComposerView>) {
    if let Some(t) = c.sched.text.clone() { let v = v.to_string(); t.update(cx, |st, cx| st.set_value(v, window, cx)); }
}
fn set_at(c: &mut ComposerView, v: &str, window: &mut Window, cx: &mut Context<ComposerView>) {
    if let Some(t) = c.sched.at.clone() { let v = v.to_string(); t.update(cx, |st, cx| st.set_value(v, window, cx)); }
}

/// The clock: the panel opens (the composer's text, At a time, now + 30
/// minutes, the list loading) or closes. Nothing of the form is kept.
pub fn toggle(c: &mut ComposerView, window: &mut Window, cx: &mut Context<ComposerView>) {
    if c.sched.open { c.sched.open = false; cx.notify(); return; }
    if c.sched.text.is_none() {
        c.sched.text = Some(cx.new(|cx| w::TextareaState::new(window, cx).auto_grow(3, 3).placeholder("Message to send later…")));
        c.sched.at = Some(cx.new(|cx| w::InputState::new(window, cx).placeholder("mm/dd/yyyy, --:-- --")));
    }
    let draft = c.text(cx);
    set_text(c, &draft, window, cx);
    set_at(c, &rules::default_at(chrono::Local::now()), window, cx);
    let st = &mut c.sched;
    st.open = true; st.trigger = "time".into(); st.note.clear(); st.items.clear();
    st.warm = None; st.list.clear(); st.unsupported = false; st.loaded = false;
    if let Some(t) = c.sched.text.clone() { t.update(cx, |st, cx| st.focus(window, cx)); }
    load(c, cx);
    cx.notify();
}

fn close(c: &mut ComposerView, cx: &mut Context<ComposerView>) { c.sched.open = false; cx.notify(); }

/// The list (this chat's pending items) and whether the warmer is on.
fn load(c: &mut ComposerView, cx: &mut Context<ComposerView>) {
    let Some(api) = crundi_shell::api(cx) else { return };
    let chat = c.chat_id(cx);
    let path = format!("/api/chat-schedule?project={}&chat={}", enc(&crundi_shell::project(cx)), enc(&chat));
    let stub = crundi_core::driver::take_stub("sched-list");
    let me = cx.entity().downgrade();
    crundi_shell::run(cx, move || match stub { Some(v) => stub_result(v), None => api.get(&path) }, move |r, cx| {
        if let Err(ApiError::SignedOut) = r { return crundi_shell::session::sign_out(cx, "Your sign-in ended. Sign in again."); }
        let _ = me.update(cx, |c, cx| {
            let sc = &mut c.sched;
            sc.loaded = true;
            match r {
                Ok(v) => {
                    sc.items = v.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default().iter().enumerate()
                        .map(|(i, it)| Item { id: s(it, "id"), when: rules::when_label(it), text: s(it, "text"), next: i == 0 }).collect();
                    sc.list = if sc.items.is_empty() { "Nothing scheduled for this project.".into() } else { String::new() };
                    let warm = v.get("limitWarmup").and_then(|w| w.as_bool()).unwrap_or(false);
                    sc.warm = Some(warm);
                    if !warm && sc.trigger == "limit-reset" { sc.trigger = "time".into(); sc.note.clear(); }
                }
                // An older server: no limit warmer to speak of either.
                Err(ApiError::Status(404, _)) => {
                    sc.unsupported = true; sc.warm = Some(false);
                    if sc.trigger == "limit-reset" { sc.trigger = "time".into(); }
                    sc.list = "Scheduled messages are not supported by this server.".into(); sc.items.clear();
                }
                Err(_) => { sc.list = "Could not load scheduled messages.".into(); sc.items.clear(); }
            }
            cx.notify();
        });
    });
}

fn set_trigger(c: &mut ComposerView, t: &str, cx: &mut Context<ComposerView>) {
    if t == "limit-reset" && c.sched.warm == Some(false) { return; }
    c.sched.trigger = t.to_string();
    c.sched.note = rules::trigger_note(t).to_string();
    cx.notify();
}

/// Schedule: checked here (an empty message, no time), then the server's
/// word. Done: the text and note clear (trigger and time stay), the
/// composer empties if it holds the same text, the list reloads.
fn save(c: &mut ComposerView, cx: &mut Context<ComposerView>) {
    if c.sched.unsupported { return; }
    let (text, at) = (text_of(c, cx), at_of(c, cx));
    let trigger = match rules::check(&text, &c.sched.trigger, &at) {
        Err(n) => { c.sched.note = n.into(); cx.notify(); return; }
        Ok(t) => t,
    };
    let chat = c.chat_id(cx);
    let sent = text.trim().to_string();
    let body = json!({ "project": crundi_shell::project(cx), "text": sent, "trigger": trigger, "chat": chat });
    let Some(api) = crundi_shell::api(cx) else { return };
    let stub = crundi_core::driver::take_stub("sched-save");
    let me = cx.entity().downgrade();
    crundi_shell::run(cx, move || match stub { Some(v) => stub_result(v), None => api.post("/api/chat-schedule", &body) }, move |r, cx| {
        if let Err(ApiError::SignedOut) = r { return crundi_shell::session::sign_out(cx, "Your sign-in ended. Sign in again."); }
        let note = match &r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Some({ let e = s(v, "error"); if e.is_empty() { "Could not schedule that.".to_string() } else { e } }),
            Ok(_) => None,
            Err(ApiError::Status(404, _)) => Some("Not supported by this server.".into()),
            Err(ApiError::Status(_, e)) if !e.is_empty() => Some(e.clone()),
            Err(ApiError::Status(..)) => Some("Could not schedule that.".into()),
            Err(e) => Some(format!("Could not schedule that: {e}")),
        };
        let Some(c) = me.upgrade() else { return };
        crate::attach::in_window(cx, move |window, cx| c.update(cx, |c, cx| {
            match note {
                Some(n) => c.sched.note = n,
                None => {
                    set_text(c, "", window, cx);
                    c.sched.note.clear();
                    // The composer, holding what was scheduled, empties.
                    if c.text(cx).trim() == sent { c.set_text("", window, cx); }
                    load(c, cx);
                }
            }
            cx.notify();
        }));
    });
}

/// ✕ on an item: no confirmation; the list reloads whatever the answer.
fn delete(_c: &mut ComposerView, id: String, cx: &mut Context<ComposerView>) {
    let me = cx.entity().downgrade();
    crundi_shell::call(cx, move |api| api.delete(&format!("/api/chat-schedule/{}", enc(&id))), move |_, cx| {
        let _ = me.update(cx, |c, cx| load(c, cx));
    });
}

// ─── What it draws ───

/// The clock (meta row; `narrow`: the box row's control-sized square).
pub fn clock_button(_c: &mut ComposerView, narrow: bool, cx: &mut Context<ComposerView>) -> AnyElement {
    let (size, icon) = if narrow { (T::CONTROL, T::ICON_LG) } else { (T::SP_24, T::ICON_MD) };
    w::square_button(if narrow { "schedule-n" } else { "schedule" }, IconName::Clock, "Schedule this message", w::SquareLook::Bare, size, icon)
        .on_click(cx.listener(|this, _, window, cx| toggle(this, window, cx)))
        .into_any_element()
}

/// The panel, just above the composer (absolute in it, growing up
/// over the log to a share of the pane).
pub fn render(c: &mut ComposerView, _window: &mut Window, cx: &mut Context<ComposerView>) -> Option<AnyElement> {
    if !c.sched.open { return None; }
    let (Some(text), Some(at)) = (c.sched.text.clone(), c.sched.at.clone()) else { return None };
    let sc = &c.sched;
    let chat = c.chat_id(cx);
    // Its tallest: 82% of the pane (Slint's sched-share).
    let pane_h = crundi_workbench::workbench(cx).map(|w| w.update(cx, |w, cx| w.panes(cx))).unwrap_or_default().iter()
        .find(|p| p.get("key").and_then(|k| k.as_str()) == Some(&format!("live:{chat}"))).and_then(|p| p.get("h")).and_then(|h| h.as_f64()).unwrap_or(600.0) as f32;
    // Above the composer (its bottom on the composer's top): the pane less
    // its head, the composer and a gap, at most 82% of the pane (Slint's sched-share).
    let comp_h = cx.try_global::<crate::attach::Media>().and_then(|m| m.bounds.get(&chat)).map(|b| f32::from(b.size.height)).unwrap_or(0.0);
    let max_h = px((pane_h * 0.82).min(pane_h - comp_h - f32::from(T::PANE_HEAD) - f32::from(T::SP_16)));
    let trig = |id: &'static str, label: &'static str, key: &'static str, off: bool, cx: &mut Context<ComposerView>| {
        let b = w::disabled(w::toggle(id, label, sc.trigger == key), off).w_full();
        let b = if off { b.tooltip("Needs the limit warmer, which is off") } else { b };
        div().flex_1().child(b.on_click(cx.listener(move |this, _, _, cx| set_trigger(this, key, cx))))
    };
    let limit_off = sc.warm == Some(false);
    let mut list = div().flex().flex_col().gap(T::SP_6);
    if !sc.list.is_empty() { list = list.child(div().text_size(T::TEXT_12).italic().text_color(T::text_muted()).child(sc.list.clone())); }
    for (i, it) in sc.items.iter().enumerate() {
        let id = it.id.clone();
        list = list.child(div().relative().flex().flex_col().gap(T::SP_2).py(T::SP_6).pl(T::SP_10).pr(T::SP_24)
            .rounded(T::R_CONTROL).border_1().border_color(if it.next { T::accent() } else { T::border_subtle() }).bg(T::bg_secondary())
            .child(div().flex().font_family(T::MONO).text_size(T::TEXT_11)
                .when(it.next, |d| d.child(div().font_weight(T::W_BOLD).text_color(T::accent_hover()).child("next")).child(div().text_color(T::text_muted()).child(" · ")))
                .child(div().text_color(T::text_muted()).child(it.when.clone())))
            .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).line_clamp(3).child(it.text.clone()))
            .child(div().absolute().top(T::SP_4).right(T::SP_4)
                .child(w::icon_button(("sched-del", i), IconName::X, "Cancel this message").on_click(cx.listener(move |this, _, _, cx| delete(this, id.clone(), cx))))));
    }
    let unsupported = sc.unsupported;
    let body = div().id("sched-body").flex().flex_col().gap(T::SP_8).p(T::SP_10).overflow_y_scroll().flex_shrink(1.).min_h_0()
        .child(w::textarea(&text))
        .child(div().flex().gap(T::SP_6)
            .child(trig("sched-time", "At a time", "time", false, cx))
            .child(trig("sched-turn", "Turn ends", "turn-end", false, cx))
            .child(trig("sched-limit", "Limit resets", "limit-reset", limit_off, cx)))
        .when(sc.trigger == "time", |d| d.child(w::input(&at)))
        .when(!sc.note.is_empty(), |d| d.child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(sc.note.clone())))
        .child(w::disabled(w::primary("sched-save", "Schedule"), unsupported).w_full().on_click(cx.listener(|this, _, _, cx| save(this, cx))))
        .child(w::rule())
        .child(list);
    // Drawn after the composer's own row (deferred), so the box never shows through.
    Some(deferred(div().id("sched-panel").absolute().left(T::SP_8).right(T::SP_8).bottom(relative(1.)).mb(T::SP_8).max_h(max_h)
        .flex().flex_col().bg(T::bg()).rounded(T::R_CARD).border_1().border_color(T::border()).shadow_lg().occlude()
        .child(div().flex().flex_none().items_center().py(T::SP_8).px(T::SP_10).border_b_1().border_color(T::border_subtle())
            .child(div().flex_1().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).child("Schedule a message"))
            .child(w::icon_button("sched-x", IconName::X, "Close").on_click(cx.listener(|this, _, _, cx| close(this, cx)))))
        .child(body)).with_priority(1)
        .into_any_element())
}

// ─── The test driver ───

pub fn test_ui(c: &mut ComposerView, name: &str, args: &[String], window: &mut Window, cx: &mut Context<ComposerView>) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    match name {
        "sched-open" => toggle(c, window, cx),
        "sched-close" => close(c, cx),
        "sched-trigger" => set_trigger(c, &a(0), cx),
        "sched-text" => set_text(c, &a(0), window, cx),
        "sched-at" => set_at(c, &a(0), window, cx),
        "sched-save" => save(c, cx),
        "sched-delete" => delete(c, a(0), cx),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

pub fn pane_props(c: &ComposerView, cx: &App) -> Value {
    let sc = &c.sched;
    if sc.text.is_none() { return json!({ "sched": null }); }
    json!({ "sched": {
        "open": sc.open, "text": text_of(c, cx), "trigger": sc.trigger, "at": at_of(c, cx), "note": sc.note, "list": sc.list,
        "limitOff": sc.warm == Some(false), "unsupported": sc.unsupported, "loaded": sc.loaded,
        "items": sc.items.iter().map(|i| json!({ "id": i.id, "when": i.when, "text": i.text, "next": i.next })).collect::<Vec<_>>(),
    }})
}

pub fn props(_c: &ComposerView, _cx: &App) -> Value { json!({}) }
