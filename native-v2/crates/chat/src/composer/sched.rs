//! 2.10: the schedule button and its panel (the web's .cc-sched and
//! .cc-schedpanel, openSched): the clock (in the meta row; on the buttons'
//! row when narrow) toggles the panel over the chat's foot, not modal: a
//! head ("Schedule a message", ✕), the message (the composer's text carried
//! in), the triggers (At a time, Turn ends, Limit resets: disabled without
//! the limit warmer), the time field (now + 30 minutes), the note, Schedule,
//! then the project's pending messages for this chat (the first is "next"),
//! each with ✕. GET/POST /api/chat-schedule, DELETE /api/chat-schedule/:id;
//! the rules are crundi_core::sched's.
//!
//! The panel is a view of its own (the pane's body places it, cached, at
//! the height it measured itself, at most 82% of the chat as the web's).

use super::Composer;
use crundi_core::api::{ApiError, ApiResult};
use crundi_core::sched as rules;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};

#[derive(Clone)]
pub struct Item { pub id: String, pub when: String, pub text: String, pub next: bool }

pub struct SchedPanel {
    composer: WeakEntity<Composer>,
    chat: String,
    project: String,
    text: Entity<w::TextareaState>,
    at: Entity<w::InputState>,
    pub trigger: String,
    pub note: String,
    pub items: Vec<Item>,
    /// The limit warmer is on (the web takes it as on until the list says).
    pub warm: bool,
    /// The list's own line: "" (items), none, an error, unsupported.
    pub list: String,
    pub unsupported: bool,
    pub loaded: bool,
    /// The head's and the body's content heights, as laid out.
    head_h: Pixels,
    body_h: Pixels,
}

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

fn enc(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// A test stub as a server answer (the driver's "stub"): a status of 400 or
/// more is that error, "net" a network failure, else the answer itself.
fn stub_result(v: Value) -> ApiResult {
    match v.get("status").and_then(|x| x.as_u64()) {
        Some(st) if st >= 400 => Err(ApiError::Status(st as u16, s(&v, "error"))),
        _ if v.get("net").is_some() => Err(ApiError::Net(s(&v, "net"))),
        _ => Ok(v),
    }
}

/// The clock: the panel opens (the composer's text, At a time, now + 30
/// minutes, the list loading) or closes. Nothing of the form is kept.
pub fn toggle(c: &mut Composer, window: &mut Window, cx: &mut Context<Composer>) {
    if c.sched.take().is_some() { cx.notify(); return; }
    let draft = c.text(cx);
    let me = cx.entity().downgrade();
    let (chat, project) = (c.id.clone(), crundi_shell::project(cx));
    let panel = cx.new(|cx| {
        let text = cx.new(|cx| w::TextareaState::new(window, cx).auto_grow(3, 3).placeholder("Message to send later…"));
        text.update(cx, |st, cx| st.set_value(draft, window, cx));
        let at = cx.new(|cx| w::InputState::new(window, cx));
        at.update(cx, |st, cx| st.set_value(rules::default_at(chrono::Local::now()), window, cx));
        SchedPanel { composer: me, chat, project, text, at, trigger: "time".into(), note: String::new(), items: vec![], warm: true,
            list: String::new(), unsupported: false, loaded: false, head_h: px(0.), body_h: px(0.) }
    });
    let t = panel.read(cx).text.clone();
    t.update(cx, |st, cx| st.focus(window, cx));
    panel.update(cx, |p, cx| p.load(cx));
    c.sched = Some((panel, T::DIALOG_MAX_H));
    cx.notify();
}

fn close_panel(composer: &WeakEntity<Composer>, cx: &mut App) {
    let _ = composer.update(cx, |c, cx| if c.sched.take().is_some() { cx.notify(); });
}

impl SchedPanel {
    /// The list (this chat's pending items) and whether the warmer is on.
    fn load(&mut self, cx: &mut Context<Self>) {
        let Some(api) = crundi_shell::api(cx) else { return };
        if self.project.is_empty() { return; }
        let path = format!("/api/chat-schedule?project={}&chat={}", enc(&self.project), enc(&self.chat));
        let stub = crundi_core::driver::take_stub("sched-list");
        let me = cx.entity().downgrade();
        crundi_shell::run(cx, move || -> ApiResult { match stub { Some(v) => stub_result(v), None => api.get(&path) } }, {
            move |r: ApiResult, cx| {
                let _ = me.update(cx, |p, cx| {
                    p.loaded = true;
                    match r {
                        Ok(v) => {
                            p.items = v.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default().iter().enumerate()
                                .map(|(i, it)| Item { id: s(it, "id"), when: rules::when_label(it), text: s(it, "text"), next: i == 0 }).collect();
                            p.list = if p.items.is_empty() { "Nothing scheduled for this project.".into() } else { String::new() };
                            p.warm = v.get("limitWarmup").and_then(|w| w.as_bool()).unwrap_or(false);
                            // Armed and since turned off: back to a time.
                            if !p.warm && p.trigger == "limit-reset" { p.set_trigger("time", cx); }
                        }
                        Err(ApiError::Status(404, _)) => {
                            p.unsupported = true; p.warm = false;
                            if p.trigger == "limit-reset" { p.set_trigger("time", cx); }
                            p.list = "Scheduled messages are not supported by this server.".into(); p.items.clear();
                        }
                        Err(_) => { p.list = "Could not load scheduled messages.".into(); p.items.clear(); }
                    }
                    cx.notify();
                });
            }
        });
    }

    fn set_trigger(&mut self, t: &str, cx: &mut Context<Self>) {
        if t == "limit-reset" && !self.warm {
            self.note = "Turn on the limit warmer in Settings to use this — it is what notices the window rolling over.".into();
            return cx.notify();
        }
        self.trigger = t.to_string();
        self.note = rules::trigger_note(t).to_string();
        cx.notify();
    }

    /// Schedule: checked here (an empty message, no time), then the
    /// server's word. Done: the message and the note clear, the composer
    /// empties if it holds the same text, the list reloads.
    fn save(&mut self, cx: &mut Context<Self>) {
        if self.unsupported { return; }
        let (text, at) = (self.text.read(cx).value().to_string(), self.at.read(cx).value().to_string());
        let trigger = match rules::check(&text, &self.trigger, &at) {
            Err(n) => { self.note = n.into(); return cx.notify(); }
            Ok(t) => due_now(t, chrono::Utc::now()),
        };
        let sent = text.trim().to_string();
        let body = json!({ "project": self.project, "text": sent, "trigger": trigger, "chat": self.chat });
        let Some(api) = crundi_shell::api(cx) else { return };
        let stub = crundi_core::driver::take_stub("sched-save");
        let me = cx.entity().downgrade();
        crundi_shell::run(cx, move || -> ApiResult { match stub { Some(v) => stub_result(v), None => api.post("/api/chat-schedule", &body) } }, move |r, cx| {
            let note = match &r {
                Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Some({ let e = s(v, "error"); if e.is_empty() { "Could not schedule that.".to_string() } else { e } }),
                Ok(_) => None,
                Err(ApiError::Status(_, e)) if !e.is_empty() => Some(e.clone()),
                Err(ApiError::Status(..)) => Some("Could not schedule that.".into()),
                Err(e) => Some(format!("Could not schedule that: {e}")),
            };
            let Some(p) = me.upgrade() else { return };
            super::in_window(cx, move |window, cx| p.update(cx, |p, cx| {
                match note {
                    Some(n) => p.note = n,
                    None => {
                        p.text.update(cx, |st, cx| st.set_value("", window, cx));
                        p.note.clear();
                        // The composer, holding what was scheduled, empties.
                        let _ = p.composer.update(cx, |c, cx| if c.text(cx).trim() == sent { c.set_text("", window, cx); });
                        p.load(cx);
                    }
                }
                cx.notify();
            }));
        });
    }

    /// ✕ on an item: no confirmation; the list reloads whatever the answer.
    fn delete(&mut self, id: String, cx: &mut Context<Self>) {
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.delete(&format!("/api/chat-schedule/{}", enc(&id))), move |_, cx| {
            let _ = me.update(cx, |p, cx| p.load(cx));
        });
    }

    /// Its contents laid out: the height it wants goes to the composer (the
    /// pane's body places it at that height).
    fn measured(&mut self, head: Option<Pixels>, body: Option<Pixels>, cx: &mut Context<Self>) {
        if let Some(h) = head { self.head_h = h; }
        if let Some(b) = body { self.body_h = b; }
        // Its border, and the body's padding (sp-10 above and below).
        let want = self.head_h + self.body_h + T::SP_10 * 2. + px(2.);
        let me = cx.entity();
        let _ = self.composer.update(cx, |c, cx| if let Some((v, h)) = c.sched.as_mut() {
            if *v == me && (*h - want).abs() > px(0.5) { *h = want; cx.notify(); }
        });
    }

    pub fn state(&self, cx: &App) -> Value {
        json!({ "open": true, "text": self.text.read(cx).value().to_string(), "trigger": self.trigger, "at": self.at.read(cx).value().to_string(),
            "note": self.note, "list": self.list, "limitOff": !self.warm, "unsupported": self.unsupported, "loaded": self.loaded,
            "items": self.items.iter().map(|i| json!({ "id": i.id, "when": i.when, "text": i.text, "next": i.next })).collect::<Vec<_>>() })
    }
}

/// Reports a part's laid-out height (after the frame) to the panel.
fn measure(me: WeakEntity<SchedPanel>, was: Pixels, head: bool) -> impl IntoElement {
    canvas(move |b, window, cx| {
        let h = b.size.height;
        if (h - was).abs() > px(0.5) {
            window.defer(cx, move |_, cx| { let _ = me.update(cx, |p, cx| if head { p.measured(Some(h), None, cx) } else { p.measured(None, Some(h), cx) }); });
        }
    }, |_, _, _, _| {}).absolute().size_full()
}

impl Render for SchedPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("sched-panel");
        let me = cx.entity().downgrade();
        let trig = |id: &'static str, label: &'static str, key: &'static str, off: bool, on: bool, cx: &mut Context<Self>| {
            let b = w::toggle(id, label, on).w_full();
            let b = if off { b.opacity(T::MEDIA_O_SOFT).tooltip("Needs the limit warmer, which is off") } else { b };
            div().flex_1().min_w(T::SP_48 * 2.).child(b.on_click(cx.listener(move |this, _, _, cx| this.set_trigger(key, cx))))
        };
        let mut list = div().flex().flex_col().gap(T::SP_6);
        if !self.list.is_empty() { list = list.child(div().text_size(T::TEXT_12).italic().text_color(T::text_muted()).child(self.list.clone())); }
        for (i, it) in self.items.iter().enumerate() {
            let id = it.id.clone();
            list = list.child(div().relative().flex().flex_col().gap(T::SP_2).py(T::SP_6).pl(T::SP_10).pr(T::SP_24)
                .rounded(T::R_CONTROL).border_1().border_color(if it.next { T::accent() } else { T::border_subtle() }).bg(T::bg_secondary())
                .child(div().flex().font_family(T::MONO).text_size(T::TEXT_11)
                    .when(it.next, |d| d.child(div().font_weight(T::W_BOLD).text_color(T::accent_hover()).child("next")).child(div().text_color(T::text_muted()).child(" · ")))
                    .child(div().text_color(T::text_muted()).child(it.when.clone())))
                .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).line_clamp(3).text_ellipsis().child(it.text.clone()))
                .child(div().absolute().top(T::SP_4).right(T::SP_4)
                    .child(w::icon_button(("sched-del", i), IconName::X, "Cancel this message").on_click(cx.listener(move |this, _, _, cx| this.delete(id.clone(), cx))))));
        }
        let (on_t, on_e, on_l) = (self.trigger == "time", self.trigger == "turn-end", self.trigger == "limit-reset");
        let off = !self.warm;
        let close_me = self.composer.clone();
        div().id("sched-panel").occlude().size_full().flex().flex_col().bg(T::bg()).rounded(T::R_CARD).border_1().border_color(T::border()).shadow_md()
            .child(div().relative().flex().flex_none().items_center().py(T::SP_8).px(T::SP_10).border_b_1().border_color(T::border_subtle())
                .child(div().flex_1().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Schedule a message"))
                .child(w::icon_button("sched-x", IconName::X, "Close").on_click(move |_, _, cx| close_panel(&close_me, cx)))
                .child(measure(me.clone(), self.head_h, true)))
            .child(div().id("sched-body").flex_1().min_h_0().overflow_y_scroll().p(T::SP_10)
                .child(div().relative().flex().flex_col().gap(T::SP_8)
                    .child(w::textarea(&self.text))
                    .child(div().flex().flex_wrap().gap(T::SP_6)
                        .child(trig("sched-time", "At a time", "time", false, on_t, cx))
                        .child(trig("sched-turn", "Turn ends", "turn-end", false, on_e, cx))
                        .child(trig("sched-limit", "Limit resets", "limit-reset", off, on_l, cx)))
                    .when(on_t, |d| d.child(w::input(&self.at)))
                    .when(!self.note.is_empty(), |d| d.child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(self.note.clone())))
                    .child(w::disabled(w::primary("sched-save", "Schedule"), self.unsupported).w_full().on_click(cx.listener(|this, _, _, cx| this.save(cx))))
                    .child(list)
                    .child(measure(me, self.body_h, false))))
    }
}

/// The clock (meta row; `narrow`: the buttons' row, control-sized).
pub fn clock_button(_c: &Composer, narrow: bool, cx: &mut Context<Composer>) -> AnyElement {
    let (size, icon) = if narrow { (T::CONTROL, T::ICON_LG) } else { (T::SP_24, T::ICON_MD) };
    w::square_button(if narrow { "schedule-n" } else { "schedule" }, IconName::Clock, "Schedule this message", w::SquareLook::Bare, size, icon)
        .on_click(cx.listener(|this, _, window, cx| toggle(this, window, cx)))
        .into_any_element()
}

// ─── The test driver ───

pub fn test_ui(c: &mut Composer, name: &str, args: &[String], window: &mut Window, cx: &mut Context<Composer>) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    if name == "sched-open" { toggle(c, window, cx); return Some(json!({ "ok": true })); }
    if !name.starts_with("sched-") { return None; }
    let Some((p, _)) = c.sched.clone() else { return Some(json!({ "ok": false, "error": "the panel is closed" })) };
    match name {
        "sched-close" => { c.sched = None; cx.notify(); }
        "sched-trigger" => p.update(cx, |p, cx| p.set_trigger(&a(0), cx)),
        "sched-text" => { let t = a(0); p.update(cx, |p, cx| p.text.update(cx, |st, cx| st.set_value(t, window, cx))); }
        "sched-at" => { let t = a(0); p.update(cx, |p, cx| p.at.update(cx, |st, cx| st.set_value(t, window, cx))); }
        "sched-save" => p.update(cx, |p, cx| p.save(cx)),
        "sched-delete" => { let id = a(0); p.update(cx, |p, cx| p.delete(id, cx)); }
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

pub fn state(c: &Composer, cx: &App) -> Value {
    json!({ "sched": c.sched.as_ref().map(|(p, h)| { let mut v = p.read(cx).state(cx); v["h"] = json!(f32::from(*h)); v }) })
}

/// A time trigger already past, but by less than a minute and a half (the
/// field's minute plus a slow save), becomes now; anything else is as it was.
pub(crate) fn due_now(mut trigger: Value, now: chrono::DateTime<chrono::Utc>) -> Value {
    let at = trigger.get("at").and_then(|a| a.as_str()).and_then(|a| chrono::DateTime::parse_from_rfc3339(a).ok());
    if let Some(at) = at {
        let past = now.signed_duration_since(at.with_timezone(&chrono::Utc));
        if past > chrono::Duration::zero() && past < chrono::Duration::seconds(90) {
            trigger["at"] = json!(now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        }
    }
    trigger
}

#[cfg(test)]
mod tests {
    use super::due_now;
    use serde_json::json;

    #[test]
    fn this_minute_is_now() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-16T10:00:58Z").unwrap().with_timezone(&chrono::Utc);
        assert_eq!(due_now(json!({ "type": "time", "at": "2026-11-16T10:00:00.000Z" }), now)["at"], "2026-11-16T10:00:58.000Z");
        assert_eq!(due_now(json!({ "type": "time", "at": "2026-11-16T11:00:00.000Z" }), now)["at"], "2026-11-16T11:00:00.000Z");
        assert_eq!(due_now(json!({ "type": "turn-end" }), now), json!({ "type": "turn-end" }));
    }
}
