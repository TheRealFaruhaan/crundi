//! 2.12: "Secrets for this chat" (the web's app/vendor/crundi-chat-secrets.js;
//! native-gpui's chat-composer/secrets.rs; specs/chat-secrets.md): what the
//! pane head's lock opens for a chat: the secrets pre-authorised for it
//! (name, how long), Revoke / Revoke all, and pre-authorising more (tick
//! secrets, type each one's PIN or passphrase, a duration, Pre-authorise:
//! all or nothing). A press on the scrim, ×, Close or Esc closes it; what was
//! typed is wiped.
//!
//! v2: the server's capability and every chat's grants are one entity
//! (`Secrets`, loaded at sign-in and kept live from the `chat-secrets`
//! event); the head's count reads it (`secrets_count`) and redraws on it
//! (`secrets_entity`). The dialog is a view of its own in the modal layer.

use crundi_core::{api::Api, head, live::Event, secrets as rules, time};
use crundi_shell::Area;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

const UNSUPPORTED: &str = "This server does not support this yet. Update the server to use it.";
const DURATIONS: [(&str, &str); 4] = [("15m", "15 minutes"), ("1h", "1 hour"), ("8h", "8 hours"), ("chat", "Until this chat ends")];
const HINT: &str = "A pre-authorised secret lets this chat run commands with it without asking for the PIN or passphrase. Only to run: its value is never shown, and Claude reading a value still asks you every time. They are kept in the server\u{2019}s memory only, for this chat only.";

/// The server's capability and every chat's grants.
#[derive(Default)]
pub struct Secrets {
    /// The server can pre-authorise ("chat-secret-preauth").
    pub cap: bool,
    /// Each chat's grants (the server's list), by chat id.
    pub grants: HashMap<String, Value>,
    /// Each chat's count as last told (the expiry check notifies on a change).
    counts: HashMap<String, usize>,
}

struct Handle(Entity<Secrets>);
impl Global for Handle {}

/// The entity the head observes to redraw its lock.
pub fn secrets_entity(cx: &mut App) -> Entity<Secrets> {
    if let Some(h) = cx.try_global::<Handle>() { return h.0.clone(); }
    let e = cx.new(|_| Secrets::default());
    cx.set_global(Handle(e.clone()));
    e
}

fn chat_listed(chat: &str, cx: &App) -> Option<Value> {
    let s = crundi_shell::stores(cx);
    let p = s.projects.read(cx);
    p.terminals.iter().find(|t| t.get("id").and_then(|i| i.as_str()) == Some(chat)).cloned()
}

/// A collaborator's chat: no lock.
fn collaborator(chat: &str, cx: &App) -> bool {
    chat_listed(chat, cx).and_then(|c| c.get("collaborator").cloned()).is_some_and(|v| v.as_bool().unwrap_or(!v.is_null()))
}

/// The head's lock: None when it does not show (no capability, a
/// collaborator's chat); else the chat's grants still running.
pub fn secrets_count(chat: &str, cx: &App) -> Option<usize> {
    let s = cx.try_global::<Handle>()?.0.read(cx);
    if !s.cap || collaborator(chat, cx) { return None; }
    Some(head::grant_count(s.grants.get(chat), time::now_secs()))
}

/// The lock's tooltip for a count (the web's).
pub fn secrets_tip(n: usize) -> String { head::secrets_tip(n) }

fn set_grants(cx: &mut App, chat: &str, g: Value) {
    let chat = chat.to_string();
    secrets_entity(cx).update(cx, |s, cx| {
        if s.grants.get(&chat) == Some(&g) { return; }
        s.counts.insert(chat.clone(), head::grant_count(Some(&g), time::now_secs()));
        s.grants.insert(chat, g);
        cx.notify();
    });
}

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

/// A POST's status and body as they came (a refusal's body names the secret).
fn post_raw(api: &Api, path: &str, body: &Value) -> Result<(u16, Value), String> {
    let bytes = serde_json::to_vec(body).unwrap_or_default();
    let (status, text) = api.post_with_progress(path, &bytes, &|_, _| {}).map_err(|e| e.to_string())?;
    if status == 0 { return Err("Network error".into()); }
    Ok((status, serde_json::from_str(&text).unwrap_or(Value::Null)))
}

/// After sign-in: the capability (the handshake), then every chat's grants.
fn load(cx: &mut App) {
    let Some(api) = crundi_shell::api(cx) else { return };
    let base = api.base().to_string();
    crundi_shell::run(cx, move || crundi_core::api::handshake(&base).ok().and_then(|h| h.pointer("/server/capabilities").cloned()), |caps, cx| {
        let on = caps.as_ref().and_then(|c| c.as_array().map(|c| c.iter().any(|x| x == "chat-secret-preauth"))).unwrap_or(false);
        secrets_entity(cx).update(cx, |s, cx| if s.cap != on { s.cap = on; cx.notify(); });
        if !on { return; }
        crundi_shell::call(cx, |api| api.get("/api/secrets/chat"), |r, cx| {
            if let Some(chats) = r.ok().and_then(|v| v.get("chats").and_then(|c| c.as_object()).cloned()) {
                for (sid, list) in chats { set_grants(cx, &sid, list); }
            }
        });
    });
}

/// The part (lib.rs's init, through composer::init): sign-in loads, the
/// live event keeps it, a grant running out redraws the count (checked
/// every 30 s; nothing is notified unless a count changed).
pub fn init(cx: &mut App) {
    let _ = secrets_entity(cx);
    crundi_shell::register(cx, Area::new("chat-secrets")
        .on_batch(|_, batch, cx| for e in batch {
            let Event::ChatSecrets(v) = e else { continue };
            let sid = s(v, "sessionId");
            if sid.is_empty() { continue; }
            // A grant on its way for this chat: its answer brings the list.
            if dialog(cx).is_some_and(|d| { let d = d.read(cx); d.busy && d.chat == sid }) { continue; }
            set_grants(cx, &sid, v.get("grants").cloned().unwrap_or(Value::Null));
        })
        .on_signed_in(|_, cx| load(cx))
        .on_signed_out(|_, cx| {
            secrets_entity(cx).update(cx, |s, cx| { *s = Secrets::default(); cx.notify(); });
            super::in_window(cx, |window, cx| if dialog(cx).is_some() { w::close_dialog(window, cx); });
        })
        .test_ui(|_, name, args, window, cx| test_ui(name, args, window, cx))
        .state(|_, _, cx| state(cx)));
    cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor().timer(Duration::from_secs(30)).await;
        let _ = cx.update(|cx| {
            let e = secrets_entity(cx);
            e.update(cx, |s, cx| {
                let now = time::now_secs();
                let fresh: HashMap<String, usize> = s.grants.iter().map(|(k, g)| (k.clone(), head::grant_count(Some(g), now))).collect();
                if fresh != s.counts { s.counts = fresh; cx.notify(); }
            });
            if let Some(d) = dialog(cx) { d.update(cx, |_, cx| cx.notify()); }
        });
    }).detach();
}

// ─── The dialog ───

struct Field { input: Entity<w::InputState>, _sub: Subscription }

pub struct SecretsDialog {
    chat: String,
    /// The secrets to pick (id, name, lock); None while loading.
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
    _sub: Subscription,
}

struct Open(WeakEntity<SecretsDialog>);
impl Global for Open {}

fn dialog(cx: &App) -> Option<Entity<SecretsDialog>> { cx.try_global::<Open>().and_then(|o| o.0.upgrade()).filter(|_| w::modal_open(cx)) }

/// The head's lock: the dialog for this chat (opening it again replaces
/// it). The grants known now at once; then the chat's own and the secrets
/// to pick, from the server.
pub fn open_secrets(chat: &str, window: &mut Window, cx: &mut App) {
    if dialog(cx).is_some() { w::close_dialog(window, cx); }
    let store = secrets_entity(cx);
    let cap = store.read(cx).cap;
    let chat = chat.to_string();
    let d = cx.new(|cx| SecretsDialog {
        chat: chat.clone(), secrets: None, chosen: vec![], fields: HashMap::new(), bad: String::new(), duration: "1h".into(), busy: false,
        err: if cap { String::new() } else { UNSUPPORTED.into() }, revoking: HashSet::new(), unsupported: !cap, focus: cx.focus_handle(),
        _sub: cx.observe(&store, |_, _, cx| cx.notify()),
    });
    cx.set_global(Open(d.downgrade()));
    let focus = d.read(cx).focus.clone();
    let weak = d.downgrade();
    w::modal(window, cx, T::DIALOG_W_LG, d.clone().into(), move |window, cx| {
        // Closed: everything typed overwritten.
        let _ = weak.update(cx, |d, cx| { for f in d.fields.values() { f.input.update(cx, |s, cx| s.set_value("", window, cx)); } d.fields.clear(); });
        cx.remove_global::<Open>();
    });
    window.focus(&focus, cx);
    if !cap { return; }
    let (w1, w2) = (d.downgrade(), d.downgrade());
    let id = chat.clone();
    crundi_shell::call(cx, move |api| api.get(&format!("/api/secrets/chat?sessionId={id}")), move |r, cx| match r {
        Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => { let _ = w1.update(cx, |d, cx| { d.err = s(&v, "error"); cx.notify(); }); }
        Ok(v) => set_grants(cx, &chat, v.get("grants").cloned().unwrap_or(Value::Null)),
        Err(e) => { if e != "Not supported by this server" { let _ = w1.update(cx, |d, cx| { d.err = e; cx.notify(); }); } }
    });
    crundi_shell::call(cx, |api| api.get("/api/secrets"), move |r, cx| {
        let list = r.ok().and_then(|v| v.get("secrets").and_then(|x| x.as_array()).cloned()).unwrap_or_default().iter()
            .map(|x| (s(x, "id"), s(x, "name"), { let l = s(x, "lock"); if l.is_empty() { "passphrase".into() } else { l } })).collect();
        let _ = w2.update(cx, |d, cx| { d.secrets = Some(list); cx.notify(); });
    });
}

fn close(window: &mut Window, cx: &mut App) { if dialog(cx).is_some() { w::close_dialog(window, cx); } }

/// The grants of a chat that have not run out, as (id, name, when).
fn grants_of(chat: &str, cx: &App) -> Vec<(String, String, String)> {
    let now = chrono::Local::now();
    let now_s = time::now_secs();
    let Some(h) = cx.try_global::<Handle>() else { return vec![] };
    h.0.read(cx).grants.get(chat).and_then(|g| g.as_array()).cloned().unwrap_or_default().iter()
        .filter(|g| match g.get("expiresAt").and_then(|e| e.as_str()) { Some(at) if !at.is_empty() => time::parse_iso(at).map(|t| t > now_s).unwrap_or(true), _ => true })
        .map(|g| (s(g, "secretId"), s(g, "name"), rules::when(g.get("expiresAt").and_then(|e| e.as_str()), now)))
        .collect()
}

impl SecretsDialog {
    fn lock_of(&self, id: &str) -> String { self.secrets.iter().flatten().find(|x| x.0 == id).map(|x| x.2.clone()).unwrap_or_default() }

    /// Tick or untick a secret: its field appears and takes the focus, or
    /// goes with what was typed in it.
    fn tick(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.chosen.iter().position(|x| x == id) {
            self.chosen.remove(i);
            if self.bad == id { self.bad.clear(); }
            if let Some(f) = self.fields.remove(id) { f.input.update(cx, |s, cx| s.set_value("", window, cx)); }
            window.focus(&self.focus, cx);
        } else {
            self.chosen.push(id.to_string());
            let lock = self.lock_of(id);
            let input = cx.new(|cx| w::InputState::new(window, cx).masked(true).placeholder(if lock == "pin" { "PIN" } else { "Passphrase" }));
            let sub = cx.subscribe_in(&input, window, move |this: &mut Self, input, ev: &w::InputEvent, window, cx| match ev {
                w::InputEvent::PressEnter { .. } => this.grant(window, cx),
                w::InputEvent::Change => {
                    // A PIN takes digits only, at most 6.
                    let text = input.read(cx).value().to_string();
                    let kept = rules::typed(&lock, &text);
                    if kept != text { input.update(cx, |s, cx| s.set_value(kept, window, cx)); }
                    cx.notify();
                }
                _ => {}
            });
            input.update(cx, |s, cx| s.focus(window, cx));
            self.fields.insert(id.to_string(), Field { input, _sub: sub });
        }
        cx.notify();
    }

    /// Tab and Shift+Tab among the ticked secrets' fields (true: taken).
    fn next_field(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let order: Vec<Entity<w::InputState>> = self.secrets.iter().flatten().filter(|x| self.chosen.contains(&x.0)).filter_map(|x| self.fields.get(&x.0).map(|f| f.input.clone())).collect();
        let Some(i) = order.iter().position(|f| f.focus_handle(cx).is_focused(window)) else { return false };
        let n = order.len();
        let next = order[if back { (i + n - 1) % n } else { (i + 1) % n }].clone();
        next.update(cx, |s, cx| s.focus(window, cx));
        true
    }

    /// The two checks here, then the server (all or nothing).
    fn grant(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.unsupported { return; }
        if self.chosen.is_empty() { self.err = "Choose at least one secret.".into(); return cx.notify(); }
        let typed: HashMap<String, String> = self.fields.iter().map(|(k, f)| (k.clone(), f.input.read(cx).value().to_string())).collect();
        let empty = String::new();
        if let Some(bad) = self.chosen.iter().find(|id| !rules::lock_ok(&self.lock_of(id), typed.get(*id).unwrap_or(&empty))).cloned() {
            self.err = "Enter the PIN or passphrase of each secret chosen.".into();
            if let Some(f) = self.fields.get(&bad).map(|f| f.input.clone()) { f.update(cx, |s, cx| s.focus(window, cx)); }
            self.bad = bad;
            return cx.notify();
        }
        self.bad.clear();
        self.err.clear();
        self.busy = true;
        cx.notify();
        let body = json!({ "action": "grant", "sessionId": self.chat, "duration": self.duration,
            "secrets": self.chosen.iter().map(|id| json!({ "id": id, "pin": typed.get(id).cloned().unwrap_or_default() })).collect::<Vec<_>>() });
        let Some(api) = crundi_shell::api(cx) else { return };
        let chat = self.chat.clone();
        let me = cx.entity().downgrade();
        crundi_shell::run(cx, move || {
            let mut body = body;
            let r = post_raw(&api, "/api/secrets/chat", &body);
            // The locks in the body: overwritten once sent.
            if let Some(list) = body.get_mut("secrets").and_then(|l| l.as_array_mut()) { for x in list { x["pin"] = json!(""); } }
            r
        }, move |r, cx| {
            if let Ok((_, v)) = &r { if let Some(g) = v.get("grants") { set_grants(cx, &chat, g.clone()); } }
            let Some(d) = me.upgrade() else { return };
            let ok = super::in_window_ret(cx, move |window, cx| d.update(cx, |d, cx| {
                d.busy = false;
                let mut ok = false;
                match r {
                    Ok((st, v)) if st < 300 && v.get("ok").and_then(|o| o.as_bool()) == Some(true) => {
                        for f in d.fields.values() { f.input.update(cx, |s, cx| s.set_value("", window, cx)); }
                        d.fields.clear();
                        d.chosen.clear();
                        d.bad.clear();
                        window.focus(&d.focus, cx);
                        ok = true;
                    }
                    Ok((404, _)) => d.err = UNSUPPORTED.into(),
                    Ok((_, v)) => {
                        let (e, sid) = (s(&v, "error"), s(&v, "secretId"));
                        match d.secrets.iter().flatten().find(|x| x.0 == sid && !sid.is_empty()).map(|x| x.1.clone()) {
                            Some(name) => {
                                if let Some(f) = d.fields.get(&sid).map(|f| f.input.clone()) { f.update(cx, |s, cx| { s.set_value("", window, cx); s.focus(window, cx); }); }
                                d.bad = sid;
                                d.err = format!("{name}: {e}");
                            }
                            None => d.err = if e.is_empty() { "Nothing could be pre-authorised for this chat.".into() } else { e },
                        }
                    }
                    Err(e) => d.err = e,
                }
                cx.notify();
                ok
            })).unwrap_or(false);
            if ok { super::toast(cx, "Pre-authorised for this chat", false); }
        });
    }

    /// Revoke one (Some) or all (None): no confirmation; the button pending.
    fn revoke(&mut self, one: Option<String>, cx: &mut Context<Self>) {
        let tag = one.clone().unwrap_or_else(|| "*all*".into());
        if !self.revoking.insert(tag.clone()) { return; }
        cx.notify();
        let name = one.as_ref().and_then(|id| grants_of(&self.chat, cx).into_iter().find(|g| g.0 == *id)).map(|g| g.1).unwrap_or_default();
        let body = match &one { Some(id) => json!({ "action": "revoke", "sessionId": self.chat, "secretId": id }), None => json!({ "action": "revokeAll", "sessionId": self.chat }) };
        let Some(api) = crundi_shell::api(cx) else { return };
        let (all, chat, me) = (one.is_none(), self.chat.clone(), cx.entity().downgrade());
        crundi_shell::run(cx, move || post_raw(&api, "/api/secrets/chat", &body), move |r, cx| {
            let _ = me.update(cx, |d, cx| { d.revoking.remove(&tag); cx.notify(); });
            let (msg, err) = match r {
                Ok((code, v)) if code < 300 && v.get("ok").and_then(|o| o.as_bool()) == Some(true) => {
                    if let Some(g) = v.get("grants") { set_grants(cx, &chat, g.clone()); }
                    (if all { "Revoked all for this chat".to_string() } else { format!("Revoked {name}") }, false)
                }
                Ok((404, _)) => (UNSUPPORTED.to_string(), true),
                Ok((_, v)) => { let e = s(&v, "error"); (if !e.is_empty() { e } else if all { "Could not revoke them".into() } else { "Could not revoke it".into() }, true) }
                Err(e) => (e, true),
            };
            super::toast(cx, &msg, err);
        });
    }

    fn state(&self, cx: &App) -> Value {
        let typed: serde_json::Map<String, Value> = self.secrets.iter().flatten()
            .filter_map(|x| self.fields.get(&x.0).map(|f| (x.1.clone(), json!(f.input.read(cx).value().chars().count())))).collect();
        json!({ "chat": self.chat, "loading": self.secrets.is_none(), "secrets": self.secrets.iter().flatten().map(|x| x.1.clone()).collect::<Vec<_>>(),
            "chosen": self.chosen, "bad": self.bad, "typed": typed, "duration": self.duration, "busy": self.busy, "err": self.err, "unsupported": self.unsupported,
            "grants": grants_of(&self.chat, cx).into_iter().map(|g| json!({ "name": g.1, "when": g.2 })).collect::<Vec<_>>() })
    }

    fn id_by_name(&self, name: &str, cx: &App) -> String {
        self.secrets.iter().flatten().find(|x| x.1 == name).map(|x| x.0.clone())
            .or_else(|| grants_of(&self.chat, cx).into_iter().find(|g| g.1 == name).map(|g| g.0)).unwrap_or_default()
    }
}

fn label(text: &'static str) -> Div {
    div().pt(T::SP_4).text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(text.to_uppercase())
}

fn line(text: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_13).text_color(T::text_muted()).child(text.into()) }

impl Render for SecretsDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let grants = grants_of(&self.chat, cx);
        let mut list = div().flex().flex_col().gap(T::SP_6);
        if grants.is_empty() { list = list.child(line("Nothing is pre-authorised for this chat.")); }
        for (i, (id, name, when)) in grants.iter().cloned().enumerate() {
            let busy = self.revoking.contains(&id);
            list = list.child(div().flex().items_center().gap(T::SP_10).py(T::SP_8).px(T::SP_10).rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg())
                .child(div().flex().flex_col().flex_1().min_w_0()
                    .child(div().font_family(T::MONO).text_size(T::TEXT_13).text_color(T::text()).child(name))
                    .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(when)))
                .child(w::disabled(w::button(("sec-revoke", i), "Revoke"), busy).on_click(cx.listener(move |this, _, _, cx| this.revoke(Some(id.clone()), cx)))));
        }
        if grants.len() >= 2 {
            let busy = self.revoking.contains("*all*");
            list = list.child(div().flex().justify_end().child(w::disabled(w::button("sec-revoke-all", "Revoke all"), busy).text_color(T::red()).on_click(cx.listener(|this, _, _, cx| this.revoke(None, cx)))));
        }
        let pick_line = match &self.secrets { None => Some("Loading the secrets…"), Some(v) if v.is_empty() => Some("There are no secrets yet. Add them under Secrets."), _ => None };
        let mut pick = div().id("sec-pick").flex().flex_col().gap(T::SP_6).max_h(T::SEC_PICK_MAX_H).overflow_y_scroll();
        if let Some(l) = pick_line { pick = pick.child(line(l)); }
        for (i, (id, name, _)) in self.secrets.clone().unwrap_or_default().into_iter().enumerate() {
            let chosen = self.chosen.contains(&id);
            let bad = self.bad == id;
            let field = self.fields.get(&id).map(|f| f.input.clone());
            let me = cx.entity().downgrade();
            pick = pick.child(div().flex().items_center().gap(T::SP_8).py(T::SP_4).px(T::SP_2)
                .child(div().flex_1().min_w_0().font_family(T::MONO).text_size(T::TEXT_13)
                    .child(w::check(("sec-tick", i), name, chosen, move |_, window, cx| { let _ = me.update(cx, |d, cx| d.tick(&id, window, cx)); })))
                .when_some(field.filter(|_| chosen), |r, f| r.child(div().flex_none().w(T::SEC_LOCK_FIELD_W).child(w::input(&f).when(bad, |i| i.border_color(T::red()))))));
        }
        let mut chips = div().flex().flex_wrap().gap(T::SP_6);
        for (i, (key, text)) in DURATIONS.iter().enumerate() {
            chips = chips.child(w::toggle(("sec-dur", i), *text, self.duration == *key).on_click(cx.listener(move |this, _, _, cx| { this.duration = key.to_string(); cx.notify(); })));
        }
        div().id("sec-dialog").track_focus(&self.focus).flex().flex_col().w_full()
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| match e.keystroke.key.as_str() {
                // (Closing wipes the fields, an update of this view: after this one.)
                "escape" => { window.defer(cx, |window, cx| close(window, cx)); cx.stop_propagation(); }
                "tab" => if this.next_field(e.keystroke.modifiers.shift, window, cx) { cx.stop_propagation(); },
                _ => {}
            }))
            .child(div().flex().flex_col().gap(T::SP_10).p(T::SP_10)
                .child(div().flex().items_center().gap(T::SP_8)
                    .child(div().text_size(T::ICON_LG).text_color(T::accent_hover()).child(IconName::Lock))
                    .child(div().flex_1().text_size(T::TEXT_15).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Secrets for this chat"))
                    .child(w::icon_button("sec-x", IconName::X, "Close").on_click(|_, window, cx| close(window, cx))))
                .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).line_height(relative(T::LH_CHAT)).child(HINT))
                .child(label("Pre-authorised now"))
                .child(list)
                .child(label("Pre-authorise"))
                .child(pick)
                .child(chips))
            .child(div().flex_none().flex().flex_col().gap(T::SP_8).px(T::SP_10).pb(T::SP_10)
                .when(!self.err.is_empty(), |f| f.child(div().text_size(T::TEXT_12).text_color(T::red()).child(self.err.clone())))
                .child(div().flex().justify_end().gap(T::SP_8)
                    .child(w::button("sec-close", "Close").on_click(|_, window, cx| close(window, cx)))
                    .child(w::disabled(w::primary("sec-grant", if self.busy { "Checking…" } else { "Pre-authorise" }), self.busy || self.unsupported)
                        .on_click(cx.listener(|this, _, window, cx| this.grant(window, cx))))))
    }
}

// ─── The driver ───

fn test_ui(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    if name == "sec-open" {
        // The focused chat's (or the one named).
        let chat = if arg(0).is_empty() { crate::focused_chat(cx) } else { arg(0) };
        open_secrets(&chat, window, cx);
        return Some(json!({ "ok": true }));
    }
    if !name.starts_with("sec-") { return None; }
    let Some(d) = dialog(cx) else { return Some(json!({ "ok": false, "error": "no secrets dialog" })) };
    d.update(cx, |d, cx| match name {
        "sec-tick" => { let id = d.id_by_name(&arg(0), cx); if !id.is_empty() { d.tick(&id, window, cx); } }
        "sec-type" => {
            let id = d.id_by_name(&arg(0), cx);
            let t = arg(1);
            if let Some(f) = d.fields.get(&id).map(|f| f.input.clone()) { f.update(cx, |s, cx| s.set_value(t, window, cx)); }
        }
        "sec-duration" => { d.duration = arg(0); cx.notify(); }
        "sec-grant" => d.grant(window, cx),
        "sec-revoke" => { let id = d.id_by_name(&arg(0), cx); if !id.is_empty() { d.revoke(Some(id), cx); } }
        "sec-revoke-all" => d.revoke(None, cx),
        _ => {}
    });
    if name == "sec-close" { close(window, cx); }
    Some(json!({ "ok": true }))
}

/// The driver's `secretsDialog` (null when closed) and `secretsCount` (the
/// focused chat's lock).
fn state(cx: &mut App) -> Value {
    let chat = crate::focused_chat(cx);
    let dlg = dialog(cx).map(|d| d.read(cx).state(cx)).unwrap_or(Value::Null);
    json!({ "secretsDialog": dlg, "secretsCount": secrets_count(&chat, cx), "secretsCap": cx.try_global::<Handle>().is_some_and(|h| h.0.read(cx).cap) })
}
