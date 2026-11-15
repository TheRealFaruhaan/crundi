//! Ops area: The Secrets tab with the PC client's unlock flow (renderSecrets, crundi-secret-lock.js), and its badge.
//! UI: ui/ops/vault.slint (global OpsVault).
//!
//! The web's (src/webapp-html.js "Secrets", app/vendor/crundi-secret-lock.js):
//! - the tab: pending access requests (Approve opens the PIN modal, Deny at
//!   once), "Add a secret" (Save waits for a passphrase that meets the rule,
//!   typed twice), the list (Reveal / Copy / Hide, Edit, Delete);
//! - a value reaches the screen only through Reveal, with its lock typed in
//!   the PIN modal; leaving the tab (or signing out) forgets every value;
//! - Edit (capability `secret-edit`) and, after an old lock was accepted,
//!   the offer to set a passphrase (`secret-upgrade`), one dialog at a time;
//! - the Secrets tab's badge: the requests waiting (Shell.badge-secrets).
//!
//! What is typed into a lock field lives in the window's field and in a
//! request's body only; it is emptied when its dialog closes.

use crate::{areas::AreaEvent, kit, MainWindow, OpsVault, OpsVaultReq, OpsVaultSecret};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, VecModel};
use std::{cell::RefCell, collections::{HashMap, VecDeque}, rc::Rc};

pub const RULE: &str = "At least 8 characters, and not only digits";
pub const UNSUPPORTED: &str = "This server does not support this yet. Update the server to use it.";

// ─── Pure rules ───

/// A new lock: 8 characters or more, and not only digits (the server's rule).
pub fn passphrase_ok(t: &str) -> bool {
    t.chars().count() >= 8 && t.chars().any(|c| !c.is_ascii_digit())
}

/// The hint under a new passphrase and its confirmation: (text, warns).
pub fn new_lock_hint(a: &str, b: &str) -> (String, bool) {
    let rule = format!("{RULE}.");
    if a.is_empty() { return (rule, false); }
    if !passphrase_ok(a) { return (rule, true); }
    if !b.is_empty() && a != b { return ("The two do not match.".into(), true); }
    (rule, false)
}

/// The add form's Save: a name, a value, a good passphrase typed twice.
pub fn add_ok(name: &str, value: &str, a: &str, b: &str) -> bool {
    passphrase_ok(a) && a == b && !name.trim().is_empty() && !value.is_empty()
}

/// A secret's or a request's lock kind: "passphrase", else "pin".
pub fn lock_of(v: &Value) -> &'static str {
    if kit::s(v, "lock") == "passphrase" { "passphrase" } else { "pin" }
}

/// A request's meta line (.req-meta).
pub fn req_meta(r: &Value) -> String {
    let p = kit::s(r, "project");
    let why = kit::s(r, "reason");
    format!("{}{}", if p.is_empty() { String::new() } else { format!("project: {p} · ") }, if why.is_empty() { "Claude requested access".into() } else { why })
}

/// A secret's tag and its title: an old lock to update (needsUpgrade).
pub fn tag_of(s: &Value) -> (&'static str, &'static str) {
    if !kit::truthy(s.get("needsUpgrade").unwrap_or(&Value::Null)) { return ("", ""); }
    if lock_of(s) == "passphrase" { ("Old lock", "Sealed with an older, weaker key. Edit it to seal it again.") }
    else { ("Old PIN", "Locked with an old 6-digit PIN. Edit it to set a passphrase.") }
}

/// The PIN modal's words for a lock.
pub fn ask_word(lock: &str) -> &'static str { if lock == "passphrase" { "passphrase" } else { "PIN" } }

/// Edit's Save: what changes, and whether it may be sent.
pub struct EditForm<'a> { pub name0: &'a str, pub desc0: &'a str, pub lock: &'a str, pub name: &'a str, pub desc: &'a str, pub value: &'a str, pub relock: bool, pub new: &'a str, pub new2: &'a str, pub cur: &'a str }

impl EditForm<'_> {
    pub fn revalue(&self) -> bool { !self.value.is_empty() }
    pub fn meta(&self) -> bool { self.name.trim() != self.name0 || self.desc != self.desc0 }
    pub fn ok(&self) -> bool {
        let need_lock = self.relock || self.revalue();
        (!self.relock || (passphrase_ok(self.new) && self.new == self.new2))
            && (!need_lock || crate::secrets::lock_ok(self.lock, self.cur))
            && (self.relock || self.revalue() || self.meta())
            && !self.name.trim().is_empty()
    }
    /// The body Save sends: `updateMeta` when only the name or description
    /// changed (no lock needed), else `edit` with the current lock.
    pub fn body(&self, id: &str) -> Value {
        if !self.relock && !self.revalue() {
            return json!({ "action": "updateMeta", "id": id, "name": self.name.trim(), "description": self.desc });
        }
        let mut b = json!({ "action": "edit", "id": id, "pin": self.cur });
        if self.meta() { b["name"] = json!(self.name.trim()); b["description"] = json!(self.desc); }
        if self.relock { b["newPin"] = json!(self.new); }
        if self.revalue() { b["value"] = json!(self.value); }
        b
    }
}

/// A POST /api/secrets answer as the lock module reads it: a 404 or an
/// unknown action is "not supported yet"; a failure is { ok: false, error }.
pub fn answer(r: Result<Value, String>) -> Value {
    match r {
        Err(e) if e == "Not supported by this server" => json!({ "ok": false, "error": UNSUPPORTED, "unsupported": true }),
        Err(e) => json!({ "ok": false, "error": e }),
        Ok(v) if v.get("ok").and_then(|o| o.as_bool()) != Some(true) && kit::s(&v, "error").contains("Unknown secrets action") => json!({ "ok": false, "error": UNSUPPORTED, "unsupported": true }),
        Ok(v) => v,
    }
}

fn ok(v: &Value) -> bool { v.get("ok").and_then(|o| o.as_bool()) == Some(true) }
fn err_or(v: &Value, dflt: &str) -> String { let e = kit::s(v, "error"); if e.is_empty() { dflt.into() } else { e } }

// ─── State ───

enum PinJob {
    Reveal { id: String, name: String },
    Approve { req: String, secret: String, name: String, upgrade: bool },
}

struct EditJob { id: String, name: String, desc: String, lock: String }

#[derive(Clone)]
struct UpJob { id: String, name: String, lock: String, pin: String }

#[derive(Default)]
struct St {
    secrets: Vec<Value>,
    requests: Vec<Value>,
    revealed: HashMap<String, String>,
    loaded: bool,
    error: String,
    on_tab: bool,
    pin: Option<(PinJob, String)>,
    edit: Option<EditJob>,
    up: Option<UpJob>,
    queue: VecDeque<UpJob>,
    caps: Option<Vec<String>>,
    caps_wait: Vec<(String, Box<dyn FnOnce(bool)>)>,
    reqs: Option<Rc<VecModel<OpsVaultReq>>>,
    list: Option<Rc<VecModel<OpsVaultSecret>>>,
    clip: Option<arboard::Clipboard>,
}

thread_local! { static ST: RefCell<St> = RefCell::new(St::default()); }
fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { ST.with(|s| f(&mut s.borrow_mut())) }
fn g(f: impl FnOnce(OpsVault)) { if let Some(w) = kit::ui() { f(w.global::<OpsVault>()); } }

// ─── Registry ───

pub fn init(w: &MainWindow) {
    let reqs = kit::model::<OpsVaultReq>();
    let list = kit::model::<OpsVaultSecret>();
    let v = w.global::<OpsVault>();
    v.set_requests(kit::rc(&reqs));
    v.set_secrets(kit::rc(&list));
    with(|s| { s.reqs = Some(reqs); s.list = Some(list); });
    v.on_retry(load);
    v.on_approve(|id| approve(&id));
    v.on_deny(|id| deny(id.to_string()));
    v.on_reveal(|id| reveal(&id));
    v.on_hide(|id| { with(|s| s.revealed.remove(id.as_str())); render(); });
    v.on_copy(|id| copy(&id));
    v.on_edit(|id| edit(&id));
    v.on_delete(|id| delete(&id));
    v.on_add_edited(add_edited);
    v.on_add_save(add_save);
    v.on_pin_typed(|t| pin_typed(&t));
    v.on_pin_submit(pin_submit);
    v.on_pin_cancel(pin_close);
    v.on_ed_changed(ed_sync);
    v.on_ed_cur_typed(|t| { let lock = with(|s| s.edit.as_ref().map(|e| e.lock.clone())).unwrap_or_default(); let c = crate::secrets::typed(&lock, &t); if c != t.as_str() { g(|v| v.set_ed_cur(c.into())); } ed_sync(); });
    v.on_ed_toggle(|on| { g(|v| { v.set_ed_relock(on); if !on { v.set_ed_new("".into()); v.set_ed_new2("".into()); } }); ed_sync(); });
    v.on_ed_cancel(ed_close);
    v.on_ed_save(ed_save);
    v.on_up_changed(up_sync);
    v.on_up_later(up_close);
    v.on_up_go(up_go);
}

pub fn signed_in() {
    with(|s| { s.caps = None; s.loaded = false; s.error.clear(); });
    load();
}

pub fn signed_out() {
    pin_close(); ed_close(); up_close();
    with(|s| { let (r, l) = (s.reqs.clone(), s.list.clone()); *s = St { reqs: r, list: l, ..St::default() }; });
    g(|v| { v.set_add_name("".into()); v.set_add_desc("".into()); v.set_add_value("".into()); v.set_add_pin("".into()); v.set_add_pin2("".into()); });
    render();
}

pub fn project(_alias: &str) {}

pub fn tab_shown(tab: &str) {
    let was = with(|s| std::mem::replace(&mut s.on_tab, tab == "secrets"));
    if tab == "secrets" { load(); }
    else if was {
        // Leaving the Secrets tab: forget every revealed value.
        with(|s| s.revealed.clear());
        render();
    }
}

pub fn event(e: &AreaEvent) {
    match e {
        AreaEvent::Server("secret-requests", v) => {
            let list = v.get("requests").and_then(|r| r.as_array()).cloned().unwrap_or_default();
            with(|s| s.requests = list);
            render();
        }
        AreaEvent::Connected(true) => load(),
        _ => {}
    }
}

// ─── Loading and drawing ───

fn load() {
    if kit::api().is_none() { return; }
    kit::call(|api| api.get("/api/secrets"), |r| {
        with(|s| match &r {
            Ok(v) => {
                if ok(v) {
                    s.secrets = v.get("secrets").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                    s.requests = v.get("requests").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                    // A value revealed for a secret that is gone goes with it.
                    let ids: Vec<String> = s.secrets.iter().map(|x| kit::s(x, "id")).collect();
                    s.revealed.retain(|k, _| ids.contains(k));
                }
                s.loaded = true;
                s.error.clear();
            }
            Err(e) => { s.error = e.clone(); }
        });
        render();
    });
}

fn render() {
    let (reqs, rows, badge, loaded, error) = with(|s| {
        let reqs: Vec<OpsVaultReq> = s.requests.iter().map(|r| OpsVaultReq { id: kit::s(r, "id").into(), name: kit::s(r, "secretName").into(), meta: req_meta(r).into() }).collect();
        let rows: Vec<OpsVaultSecret> = s.secrets.iter().map(|x| {
            let id = kit::s(x, "id");
            let (tag, tip) = tag_of(x);
            let val = s.revealed.get(&id);
            OpsVaultSecret { id: id.as_str().into(), name: kit::s(x, "name").into(), desc: kit::s(x, "description").into(), tag: tag.into(), tip: tip.into(), revealed: val.is_some(), value: val.cloned().unwrap_or_default().into() }
        }).collect();
        let n = s.requests.len();
        (reqs, rows, if n > 0 { n.to_string() } else { String::new() }, s.loaded, s.error.clone())
    });
    let (rm, lm) = with(|s| (s.reqs.clone(), s.list.clone()));
    if let Some(m) = rm { kit::sync_rows(&m, reqs); }
    if let Some(m) = lm { kit::sync_rows(&m, rows); }
    if let Some(w) = kit::ui() {
        w.global::<crate::Shell>().set_badge_secrets(badge.into());
        let v = w.global::<OpsVault>();
        v.set_loaded(loaded);
        v.set_error(error.into());
        v.set_modal(v.get_pin_open() || v.get_ed_open() || v.get_up_open());
    }
}

/// Does the server list this capability? (Asked once per sign-in.)
fn supports(cap: &str, then: impl FnOnce(bool) + 'static) {
    let known = with(|s| s.caps.as_ref().map(|c| c.iter().any(|x| x == cap)));
    if let Some(k) = known { then(k); return; }
    let first = with(|s| { s.caps_wait.push((cap.to_string(), Box::new(then))); s.caps_wait.len() == 1 });
    if !first { return; }
    let base = kit::base();
    kit::bg(move || crate::api::handshake(&base).ok().and_then(|h| h.pointer("/server/capabilities").cloned()), |caps| {
        let list: Vec<String> = caps.and_then(|c| c.as_array().cloned()).unwrap_or_default().iter().filter_map(|x| x.as_str().map(String::from)).collect();
        let waiting = with(|s| { s.caps = Some(list.clone()); std::mem::take(&mut s.caps_wait) });
        for (cap, f) in waiting { f(list.contains(&cap)); }
    });
}

fn post(body: Value, done: impl FnOnce(Value) + Send + 'static) {
    if kit::api().is_none() { done(json!({ "ok": false, "error": "Not ready" })); return; }
    kit::call(move |api| api.post("/api/secrets", &body), move |r| done(answer(r)));
}

// ─── Add a secret ───

fn add_edited() {
    g(|v| {
        let (a, b) = (v.get_add_pin().to_string(), v.get_add_pin2().to_string());
        let (rule, warn) = new_lock_hint(&a, &b);
        v.set_add_rule(rule.into());
        v.set_add_warn(warn);
        v.set_add_ok(add_ok(&v.get_add_name(), &v.get_add_value(), &a, &b));
    });
}

fn add_save() {
    let Some(w) = kit::ui() else { return };
    let v = w.global::<OpsVault>();
    if v.get_add_busy() { return; }
    let name = v.get_add_name().trim().to_string();
    let (desc, value, pin, pin2) = (v.get_add_desc().to_string(), v.get_add_value().to_string(), v.get_add_pin().to_string(), v.get_add_pin2().to_string());
    if name.is_empty() { kit::toast_error("Name is required"); return; }
    if value.is_empty() { kit::toast_error("Value is required"); return; }
    if !passphrase_ok(&pin) { kit::toast_error("Passphrase: at least 8 characters, and not only digits"); return; }
    if pin != pin2 { kit::toast_error("The two passphrases do not match"); return; }
    v.set_add_busy(true);
    post(json!({ "action": "add", "name": name, "description": desc, "value": value, "pin": pin }), |d| {
        g(|v| v.set_add_busy(false));
        if ok(&d) {
            kit::toast_ok("Secret saved");
            // The web draws the tab again: the form comes back empty.
            g(|v| { v.set_add_name("".into()); v.set_add_desc("".into()); v.set_add_value("".into()); v.set_add_pin("".into()); v.set_add_pin2("".into()); });
            add_edited();
            load();
        } else { kit::toast_error(&err_or(&d, "Failed to save")); }
    });
}

// ─── The list ───

fn secret(id: &str) -> Option<Value> { with(|s| s.secrets.iter().find(|x| kit::s(x, "id") == id).cloned()) }

fn reveal(id: &str) {
    let Some(sec) = secret(id) else { return };
    let (lock, name) = (lock_of(&sec), kit::s(&sec, "name"));
    pin_open(PinJob::Reveal { id: id.into(), name: name.clone() }, lock, "Reveal secret", &format!("Enter the {} for \"{name}\".", ask_word(lock)), "Reveal");
}

fn copy(id: &str) {
    let text = with(|s| s.revealed.get(id).cloned());
    let Some(text) = text else { return };
    let done = with(|s| {
        if s.clip.is_none() { s.clip = arboard::Clipboard::new().ok(); }
        s.clip.as_mut().map(|c| c.set_text(text).is_ok()).unwrap_or(false)
    });
    if done { kit::toast_ok("Copied"); } else { kit::toast_error("Copy failed"); }
}

fn delete(id: &str) {
    let Some(sec) = secret(id) else { return };
    let name = kit::s(&sec, "name");
    let id = id.to_string();
    kit::confirm(kit::Confirm { title: "Delete secret", message: &format!("Delete secret \"{name}\"? This cannot be undone."), ok: "Delete", danger: true }, move |yes| {
        if !yes { return; }
        post(json!({ "action": "delete", "id": id }), move |d| {
            if ok(&d) { with(|s| s.revealed.remove(&id)); kit::toast_ok("Deleted"); load(); }
            else { kit::toast_error(&err_or(&d, "Failed to delete")); }
        });
    });
}

fn approve(req_id: &str) {
    let Some(r) = with(|s| s.requests.iter().find(|x| kit::s(x, "id") == req_id).cloned()) else { return };
    let lock = lock_of(&r);
    let name = kit::s(&r, "secretName");
    let job = PinJob::Approve { req: req_id.into(), secret: kit::s(&r, "secretId"), name: name.clone(), upgrade: kit::truthy(r.get("needsUpgrade").unwrap_or(&Value::Null)) };
    pin_open(job, lock, "Approve access", &format!("Enter the {} for \"{name}\" to release it to Claude.", ask_word(lock)), "Approve");
}

fn deny(id: String) {
    post(json!({ "action": "deny", "reqId": id }), |_| load());
}

// ─── The PIN modal ───

fn pin_open(job: PinJob, lock: &str, title: &str, sub: &str, label: &str) {
    with(|s| s.pin = Some((job, lock.to_string())));
    g(|v| {
        v.set_pin_title(title.into()); v.set_pin_sub(sub.into()); v.set_pin_label(label.into());
        v.set_pin_lock(lock.into()); v.set_pin_text("".into()); v.set_pin_error("".into()); v.set_pin_busy(false);
        v.set_pin_open(true);
    });
    render();
}

fn pin_close() {
    with(|s| s.pin = None);
    g(|v| { v.set_pin_text("".into()); v.set_pin_open(false); v.set_pin_busy(false); });
    render();
}

fn pin_typed(t: &str) {
    let lock = with(|s| s.pin.as_ref().map(|p| p.1.clone())).unwrap_or_default();
    let c = crate::secrets::typed(&lock, t);
    if c != t { g(|v| v.set_pin_text(c.into())); }
}

fn pin_submit() {
    let Some(w) = kit::ui() else { return };
    let v = w.global::<OpsVault>();
    if v.get_pin_busy() { return; }
    let Some((lock, body)) = with(|s| s.pin.as_ref().map(|(job, lock)| {
        let raw = v.get_pin_text().to_string();
        let pin = if lock == "passphrase" { raw } else { raw.trim().to_string() };
        (lock.clone(), match job {
            PinJob::Reveal { id, .. } => json!({ "action": "reveal", "id": id, "pin": pin }),
            PinJob::Approve { req, .. } => json!({ "action": "approve", "reqId": req, "pin": pin }),
        })
    })) else { return };
    let pin = kit::s(&body, "pin");
    if let Err(e) = crate::sreq::check(&lock, &pin) { v.set_pin_error(e.into()); return; }
    v.set_pin_busy(true);
    post(body, move |d| {
        g(|v| v.set_pin_busy(false));
        if !ok(&d) {
            g(|v| { v.set_pin_error(err_or(&d, "Failed").into()); v.set_pin_sel(v.get_pin_sel() + 1); });
            return;
        }
        let job = with(|s| s.pin.take()).map(|p| p.0);
        pin_close();
        let needs = kit::truthy(d.get("needsUpgrade").unwrap_or(&Value::Null));
        match job {
            Some(PinJob::Reveal { id, name }) => {
                with(|s| s.revealed.insert(id.clone(), kit::s(&d, "value")));
                render();
                if needs { upgrade(&id, &name, &lock, &pin); }
            }
            Some(PinJob::Approve { secret, name, upgrade: old, .. }) => {
                kit::toast_ok("Approved");
                load();
                if needs || old { upgrade(&secret, &name, &lock, &pin); }
            }
            None => {}
        }
    });
}

// ─── Edit secret ───

fn edit(id: &str) {
    let Some(sec) = secret(id) else { return };
    supports("secret-edit", move |yes| {
        if !yes { kit::toast_error(UNSUPPORTED); return; }
        let job = EditJob { id: kit::s(&sec, "id"), name: kit::s(&sec, "name"), desc: kit::s(&sec, "description"), lock: lock_of(&sec).into() };
        g(|v| {
            v.set_ed_lock(job.lock.as_str().into()); v.set_ed_name(job.name.as_str().into()); v.set_ed_desc(job.desc.as_str().into());
            v.set_ed_value("".into()); v.set_ed_relock(false); v.set_ed_new("".into()); v.set_ed_new2("".into()); v.set_ed_cur("".into());
            v.set_ed_err("".into()); v.set_ed_busy(false);
        });
        with(|s| s.edit = Some(job));
        g(|v| v.set_ed_open(true));
        ed_sync();
        render();
    });
}

fn ed_form<R>(f: impl FnOnce(&EditForm, &str) -> R) -> Option<R> {
    let w = kit::ui()?;
    let v = w.global::<OpsVault>();
    let (name, desc, value, new, new2, cur) = (v.get_ed_name(), v.get_ed_desc(), v.get_ed_value(), v.get_ed_new(), v.get_ed_new2(), v.get_ed_cur());
    let relock = v.get_ed_relock();
    with(|s| s.edit.as_ref().map(|e| f(&EditForm { name0: &e.name, desc0: &e.desc, lock: &e.lock, name: &name, desc: &desc, value: &value, relock, new: &new, new2: &new2, cur: &cur }, &e.id)))
}

fn ed_sync() {
    let Some((okd, hint)) = ed_form(|f, _| (f.ok(), new_lock_hint(f.new, f.new2))) else { return };
    g(|v| { v.set_ed_ok(okd); v.set_ed_hint(hint.0.into()); v.set_ed_warn(hint.1); });
}

fn ed_close() {
    let had = with(|s| s.edit.take().is_some());
    g(|v| { v.set_ed_open(false); v.set_ed_cur("".into()); v.set_ed_new("".into()); v.set_ed_new2("".into()); v.set_ed_value("".into()); v.set_ed_busy(false); });
    render();
    if had { next_up(); }
}

fn ed_save() {
    let Some(Some(body)) = ed_form(|f, id| f.ok().then(|| f.body(id))) else { return };
    let id = kit::s(&body, "id");
    g(|v| { v.set_ed_busy(true); v.set_ed_err("".into()); });
    post(body, move |d| {
        if with(|s| s.edit.as_ref().map(|e| e.id != id).unwrap_or(true)) { return; }
        if ok(&d) {
            kit::toast_ok("Secret saved");
            with(|s| s.revealed.remove(&id));
            ed_close();
            load();
            return;
        }
        let e = err_or(&d, "Could not save it.");
        let lockish = { let l = e.to_lowercase(); l.contains("incorrect") || l.contains("pin must") || l.ends_with("passphrase") };
        g(|v| { v.set_ed_busy(false); v.set_ed_err(e.into()); if lockish { v.set_ed_sel(v.get_ed_sel() + 1); } });
    });
}

// ─── Update this secret's lock ───

/// After an old lock was accepted: offer to set a passphrase now, with the
/// lock just typed (kept in this offer only). Asked only when the server
/// lists `secret-upgrade`; one dialog at a time (the others wait). Public:
/// the requests dock (src/app.rs) offers it after its Approve too.
pub fn upgrade(id: &str, name: &str, lock: &str, pin: &str) {
    if id.is_empty() || pin.is_empty() { return; }
    let job = UpJob { id: id.into(), name: name.into(), lock: lock.into(), pin: pin.into() };
    supports("secret-upgrade", move |yes| {
        if !yes { return; }
        let busy = with(|s| s.up.is_some() || s.edit.is_some());
        if busy { with(|s| s.queue.push_back(job)); } else { up_show(job); }
    });
}

fn up_show(job: UpJob) {
    let name = if job.name.is_empty() { "This secret".to_string() } else { job.name.clone() };
    let old = job.lock != "passphrase";
    with(|s| s.up = Some(job));
    g(|v| {
        v.set_up_name(name.into()); v.set_up_old_pin(old);
        v.set_up_new("".into()); v.set_up_new2("".into()); v.set_up_err("".into()); v.set_up_busy(false);
        v.set_up_open(true);
    });
    up_sync();
    render();
}

fn up_sync() {
    g(|v| {
        let (a, b) = (v.get_up_new().to_string(), v.get_up_new2().to_string());
        let (h, warn) = new_lock_hint(&a, &b);
        v.set_up_hint(h.into()); v.set_up_warn(warn);
        v.set_up_ok(passphrase_ok(&a) && a == b);
    });
}

fn up_close() {
    let had = with(|s| s.up.take().is_some());
    g(|v| { v.set_up_open(false); v.set_up_new("".into()); v.set_up_new2("".into()); v.set_up_busy(false); });
    render();
    if had { next_up(); }
}

fn next_up() {
    let next = with(|s| if s.up.is_none() && s.edit.is_none() { s.queue.pop_front() } else { None });
    if let Some(j) = next { up_show(j); }
}

fn up_go() {
    let Some(w) = kit::ui() else { return };
    let v = w.global::<OpsVault>();
    let (a, b) = (v.get_up_new().to_string(), v.get_up_new2().to_string());
    if v.get_up_busy() || !(passphrase_ok(&a) && a == b) { return; }
    let Some(job) = with(|s| s.up.clone()) else { return };
    v.set_up_busy(true);
    v.set_up_err("".into());
    let id = job.id.clone();
    post(json!({ "action": "edit", "id": job.id, "pin": job.pin, "newPin": a }), move |d| {
        if with(|s| s.up.as_ref().map(|u| u.id != id).unwrap_or(true)) { return; }
        if ok(&d) {
            kit::toast_ok("Lock updated");
            up_close();
            load();
            return;
        }
        g(|v| { v.set_up_busy(false); v.set_up_err(err_or(&d, "Could not update it.").into()); });
    });
}

// ─── The test driver ───

fn by_name(name: &str) -> String {
    with(|s| s.secrets.iter().find(|x| kit::s(x, "name") == name).map(|x| kit::s(x, "id"))).unwrap_or_default()
}
fn req_by_name(name: &str) -> String {
    with(|s| s.requests.iter().find(|x| kit::s(x, "secretName") == name).map(|x| kit::s(x, "id"))).unwrap_or_default()
}

/// The driver's "ops:<name>" actions this screen knows (None: not mine).
pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let w = kit::ui()?;
    let v = w.global::<OpsVault>();
    match name {
        "vault-add-set" => {
            let t = a(1);
            match a(0).as_str() { "name" => v.set_add_name(t.into()), "desc" => v.set_add_desc(t.into()), "value" => v.set_add_value(t.into()), "pin" => v.set_add_pin(t.into()), "pin2" => v.set_add_pin2(t.into()), _ => return Some(json!({ "ok": false })) }
            add_edited();
        }
        "vault-add-save" => add_save(),
        "vault-reveal" => reveal(&by_name(&a(0))),
        "vault-hide" => { let id = by_name(&a(0)); with(|s| s.revealed.remove(&id)); render(); }
        "vault-copy" => copy(&by_name(&a(0))),
        "vault-clipboard" => {
            let t = with(|s| { if s.clip.is_none() { s.clip = arboard::Clipboard::new().ok(); } s.clip.as_mut().and_then(|c| c.get_text().ok()) });
            return Some(json!({ "ok": true, "text": t }));
        }
        "vault-delete" => delete(&by_name(&a(0))),
        "vault-edit" => edit(&by_name(&a(0))),
        "vault-approve" => approve(&req_by_name(&a(0))),
        "vault-deny" => deny(req_by_name(&a(0))),
        "vault-pin-type" => { v.set_pin_text(a(0).into()); pin_typed(&a(0)); }
        "vault-pin-submit" => pin_submit(),
        "vault-pin-cancel" => pin_close(),
        "vault-edit-set" => {
            let t = a(1);
            match a(0).as_str() {
                "name" => v.set_ed_name(t.into()), "desc" => v.set_ed_desc(t.into()), "value" => v.set_ed_value(t.into()),
                "new" => v.set_ed_new(t.into()), "new2" => v.set_ed_new2(t.into()),
                "cur" => { v.set_ed_cur(t.as_str().into()); let lock = v.get_ed_lock().to_string(); let c = crate::secrets::typed(&lock, &t); v.set_ed_cur(c.into()); }
                _ => return Some(json!({ "ok": false })),
            }
            ed_sync();
        }
        "vault-edit-relock" => { let on = a(0) != "false"; v.set_ed_relock(on); if !on { v.set_ed_new("".into()); v.set_ed_new2("".into()); } ed_sync(); }
        "vault-edit-save" => ed_save(),
        "vault-edit-cancel" => ed_close(),
        "vault-up-offer" => { let job = UpJob { id: by_name(&a(0)), name: a(0), lock: a(1), pin: a(2) }; up_show(job); }
        "vault-up-set" => { let t = a(1); if a(0) == "new" { v.set_up_new(t.into()); } else { v.set_up_new2(t.into()); } up_sync(); }
        "vault-up-go" => up_go(),
        "vault-up-later" => up_close(),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// This screen's part of the area's props (under "vault").
pub fn props() -> Value {
    let Some(w) = kit::ui() else { return json!({}) };
    let v = w.global::<OpsVault>();
    let sh = w.global::<crate::Shell>();
    let secrets: Vec<Value> = v.get_secrets().iter().map(|x| {
        let mut o = json!({ "name": x.name.as_str(), "desc": x.desc.as_str(), "tag": x.tag.as_str(), "revealed": x.revealed });
        if x.revealed { o["value"] = json!(x.value.as_str()); }
        o
    }).collect();
    json!({
        "loaded": v.get_loaded(), "error": v.get_error().as_str(), "badge": sh.get_badge_secrets().as_str(), "modal": v.get_modal(),
        "requests": v.get_requests().iter().map(|r| json!({ "name": r.name.as_str(), "meta": r.meta.as_str() })).collect::<Vec<_>>(),
        "secrets": secrets,
        "add": { "rule": v.get_add_rule().as_str(), "warn": v.get_add_warn(), "ok": v.get_add_ok(), "name": v.get_add_name().as_str() },
        "pin": if v.get_pin_open() { json!({ "title": v.get_pin_title().as_str(), "sub": v.get_pin_sub().as_str(), "label": v.get_pin_label().as_str(), "lock": v.get_pin_lock().as_str(), "error": v.get_pin_error().as_str(), "typed": v.get_pin_text().chars().count(), "busy": v.get_pin_busy() }) } else { Value::Null },
        "edit": if v.get_ed_open() { json!({ "lock": v.get_ed_lock().as_str(), "name": v.get_ed_name().as_str(), "relock": v.get_ed_relock(), "hint": v.get_ed_hint().as_str(), "warn": v.get_ed_warn(), "ok": v.get_ed_ok(), "err": v.get_ed_err().as_str(), "busy": v.get_ed_busy() }) } else { Value::Null },
        "upgrade": if v.get_up_open() { json!({ "name": v.get_up_name().as_str(), "oldPin": v.get_up_old_pin(), "hint": v.get_up_hint().as_str(), "warn": v.get_up_warn(), "ok": v.get_up_ok(), "err": v.get_up_err().as_str() }) } else { Value::Null },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_passphrase_rule() {
        assert!(!passphrase_ok("1234567"));
        assert!(!passphrase_ok("12345678"));
        assert!(passphrase_ok("1234567a"));
        assert!(passphrase_ok("ééééééé1"));
        assert!(!passphrase_ok("abc"));
        assert_eq!(new_lock_hint("", ""), (format!("{RULE}."), false));
        assert_eq!(new_lock_hint("short", ""), (format!("{RULE}."), true));
        assert_eq!(new_lock_hint("long enough", "long"), ("The two do not match.".into(), true));
        assert_eq!(new_lock_hint("long enough", ""), (format!("{RULE}."), false));
        assert_eq!(new_lock_hint("long enough", "long enough"), (format!("{RULE}."), false));
    }

    #[test]
    fn the_add_form() {
        assert!(add_ok("N", "v", "open-sesame", "open-sesame"));
        assert!(!add_ok(" ", "v", "open-sesame", "open-sesame"));
        assert!(!add_ok("N", "", "open-sesame", "open-sesame"));
        assert!(!add_ok("N", "v", "open-sesame", "open-sesam"));
        assert!(!add_ok("N", "v", "12345678", "12345678"));
    }

    #[test]
    fn rows_read() {
        assert_eq!(req_meta(&json!({ "project": "demo", "reason": "deploy" })), "project: demo · deploy");
        assert_eq!(req_meta(&json!({})), "Claude requested access");
        assert_eq!(tag_of(&json!({ "needsUpgrade": true, "lock": "pin" })).0, "Old PIN");
        assert_eq!(tag_of(&json!({ "needsUpgrade": true, "lock": "passphrase" })).0, "Old lock");
        assert_eq!(tag_of(&json!({ "lock": "pin" })).0, "");
        assert_eq!(lock_of(&json!({ "lock": "passphrase" })), "passphrase");
        assert_eq!(lock_of(&json!({})), "pin");
        assert_eq!(ask_word("pin"), "PIN");
    }

    #[test]
    fn edit_rules_and_body() {
        let base = EditForm { name0: "A", desc0: "d", lock: "passphrase", name: "A", desc: "d", value: "", relock: false, new: "", new2: "", cur: "" };
        assert!(!base.ok(), "nothing changed");
        let meta = EditForm { name: "B ", ..base };
        assert!(meta.ok(), "a new name needs no lock");
        assert_eq!(meta.body("x"), json!({ "action": "updateMeta", "id": "x", "name": "B", "description": "d" }));
        let val = EditForm { value: "v2", ..base };
        assert!(!val.ok(), "a new value needs the lock");
        let val = EditForm { value: "v2", cur: "pass", ..base };
        assert!(val.ok());
        assert_eq!(val.body("x"), json!({ "action": "edit", "id": "x", "pin": "pass", "value": "v2" }));
        let re = EditForm { relock: true, new: "new passphrase", new2: "new passphrase", cur: "pass", ..base };
        assert!(re.ok());
        assert_eq!(re.body("x")["newPin"], json!("new passphrase"));
        let bad = EditForm { relock: true, new: "short", new2: "short", cur: "pass", ..base };
        assert!(!bad.ok());
        let pin = EditForm { lock: "pin", value: "v", cur: "12345", ..base };
        assert!(!pin.ok());
        let pin = EditForm { lock: "pin", value: "v", cur: "123456", ..base };
        assert!(pin.ok());
        let empty = EditForm { name: " ", ..base };
        assert!(!empty.ok());
    }

    #[test]
    fn answers() {
        assert_eq!(answer(Err("Not supported by this server".into()))["error"], json!(UNSUPPORTED));
        assert_eq!(answer(Ok(json!({ "ok": false, "error": "Unknown secrets action: edit" })))["error"], json!(UNSUPPORTED));
        assert_eq!(answer(Ok(json!({ "ok": false, "error": "Incorrect passphrase" })))["error"], json!("Incorrect passphrase"));
        assert_eq!(answer(Err("Cannot reach the server: x".into()))["ok"], json!(false));
        assert_eq!(answer(Ok(json!({ "ok": true, "value": "v" })))["value"], json!("v"));
    }
}
