//! Secrets (the Secrets tab, the PC client's unlock flow; Slint: src/ops/vault.rs,
//! ui/ops/vault.slint, ui/ops/vault-parts.slint, specs/ops/vault.md; the
//! web's renderSecrets / openPinModal and app/vendor/crundi-secret-lock.js).
//!
//! - The tab: pending access requests (Approve opens the PIN modal, Deny at
//!   once), "Add a secret" (Save waits for a passphrase that meets the rule,
//!   typed twice), the list (Reveal / Copy / Hide, Edit, Delete).
//! - A value reaches the screen only through Reveal, with its lock typed in
//!   the PIN modal; leaving the tab (or signing out) forgets every value.
//! - Edit (capability `secret-edit`) and, after an old lock was accepted,
//!   the offer to set a passphrase (`secret-upgrade`), one dialog at a time.
//! - The Secrets tab's badge: the requests waiting (`badge`, live from the
//!   `secret-requests` event; the shell draws it, shared/requests.md).
//!
//! What is typed into a lock field lives in its text box and in a request's
//! body only; it is emptied when its dialog closes.
//!
//! The screen is one entity in a Global (`Vault`); every click and driver
//! action is a free function here, which updates it through `with`.

mod rules;

use crate::compat::AreaEvent;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use rules::{add_ok, ask_word, err_or, lock_of, new_lock_hint, ok, passphrase_ok, req_meta, s, tag_of, truthy, EditForm, UNSUPPORTED};
use serde_json::{json, Value};
use std::{collections::{HashMap, VecDeque}, rc::Rc};

const TAB: &str = "secrets";

// ─── State ───

enum PinJob {
    Reveal { id: String, name: String },
    Approve { req: String, secret: String, name: String, upgrade: bool },
}

/// The PIN modal (the web's openPinModal): for what, its words, its state.
struct Pin { job: PinJob, lock: String, title: String, sub: String, label: String, error: String, busy: bool }

struct EditJob { id: String, name: String, desc: String, lock: String }
struct Edit { job: EditJob, relock: bool, err: String, busy: bool }

#[derive(Clone)]
struct UpJob { id: String, name: String, lock: String, pin: String }
struct Up { job: UpJob, err: String, busy: bool }

type Line = Entity<w::InputState>;
type Area = Entity<w::TextareaState>;

/// The screen's text boxes (made once a window is at hand).
struct Inputs {
    add_name: Line, add_desc: Area, add_value: Area, add_pin: Line, add_pin2: Line,
    pin: Line,
    ed_name: Line, ed_desc: Area, ed_value: Area, ed_new: Line, ed_new2: Line, ed_cur: Line,
    up_new: Line, up_new2: Line,
    _subs: Vec<Subscription>,
}

type CapWait = (String, Box<dyn FnOnce(bool, &mut App)>);

pub struct Vault {
    secrets: Vec<Value>,
    requests: Vec<Value>,
    revealed: HashMap<String, String>,
    loaded: bool,
    error: String,
    on_tab: bool,
    pin: Option<Pin>,
    edit: Option<Edit>,
    up: Option<Up>,
    queue: VecDeque<UpJob>,
    caps: Option<Vec<String>>,
    caps_wait: Vec<CapWait>,
    add_busy: bool,
    /// The last text Copy put on the clipboard (the driver's fallback read).
    copied: String,
    inputs: Option<Inputs>,
    focus: FocusHandle,
    scroll: ScrollHandle,
}

impl Vault {
    fn new(cx: &mut Context<Self>) -> Self {
        Vault {
            secrets: vec![], requests: vec![], revealed: HashMap::new(), loaded: false, error: String::new(), on_tab: false,
            pin: None, edit: None, up: None, queue: VecDeque::new(), caps: None, caps_wait: vec![], add_busy: false,
            copied: String::new(), inputs: None, focus: cx.focus_handle(), scroll: ScrollHandle::new(),
        }
    }
    fn modal(&self) -> bool { self.pin.is_some() || self.edit.is_some() || self.up.is_some() }
    fn badge(&self) -> String { let n = self.requests.len(); if n > 0 { n.to_string() } else { String::new() } }
    fn secret(&self, id: &str) -> Option<Value> { self.secrets.iter().find(|x| s(x, "id") == id).cloned() }
    fn by_name(&self, name: &str) -> String { self.secrets.iter().find(|x| s(x, "name") == name).map(|x| s(x, "id")).unwrap_or_default() }
    fn req_by_name(&self, name: &str) -> String { self.requests.iter().find(|x| s(x, "secretName") == name).map(|x| s(x, "id")).unwrap_or_default() }
}

fn lv(e: &Line, cx: &App) -> String { e.read(cx).value().to_string() }
fn av(e: &Area, cx: &App) -> String { e.read(cx).value().to_string() }

/// The add form's typed values: name, description, value, passphrase twice.
fn add_vals(i: &Inputs, cx: &App) -> (String, String, String, String, String) {
    (lv(&i.add_name, cx), av(&i.add_desc, cx), av(&i.add_value, cx), lv(&i.add_pin, cx), lv(&i.add_pin2, cx))
}

/// Edit's form, read from its boxes.
fn ed_form<R>(v: &Vault, cx: &App, f: impl FnOnce(&EditForm, &str) -> R) -> Option<R> {
    let (e, i) = (v.edit.as_ref()?, v.inputs.as_ref()?);
    let (name, desc, value, new, new2, cur) = (lv(&i.ed_name, cx), av(&i.ed_desc, cx), av(&i.ed_value, cx), lv(&i.ed_new, cx), lv(&i.ed_new2, cx), lv(&i.ed_cur, cx));
    Some(f(&EditForm { name0: &e.job.name, desc0: &e.job.desc, lock: &e.job.lock, name: &name, desc: &desc, value: &value, relock: e.relock, new: &new, new2: &new2, cur: &cur }, &e.job.id))
}

impl Inputs {
    fn new(window: &mut Window, cx: &mut Context<Vault>) -> Inputs {
        fn line(ph: &'static str, masked: bool, window: &mut Window, cx: &mut Context<Vault>) -> Line {
            cx.new(|cx| w::InputState::new(window, cx).placeholder(ph).masked(masked))
        }
        fn area(ph: &'static str, rows: usize, window: &mut Window, cx: &mut Context<Vault>) -> Area {
            cx.new(|cx| w::TextareaState::new(window, cx).rows(rows).placeholder(ph))
        }
        let i = Inputs {
            add_name: line("Name (e.g. AWS Access Key)", false, window, cx),
            add_desc: area("Description \u{2014} Claude searches names & descriptions", 2, window, cx),
            add_value: area("Secret value", 2, window, cx),
            add_pin: line("Passphrase to encrypt this secret", true, window, cx),
            add_pin2: line("Confirm the passphrase", true, window, cx),
            pin: line("Passphrase", true, window, cx),
            ed_name: line("Name", false, window, cx),
            ed_desc: area("Description", 3, window, cx),
            ed_value: area("Leave empty to keep the value", 3, window, cx),
            ed_new: line("New passphrase", true, window, cx),
            ed_new2: line("Confirm the passphrase", true, window, cx),
            ed_cur: line("Passphrase", true, window, cx),
            up_new: line("New passphrase", true, window, cx),
            up_new2: line("Confirm the passphrase", true, window, cx),
            _subs: vec![],
        };
        let mut subs = vec![];
        // Every box redraws the screen as it is typed in (the rule lines, Save).
        for l in [&i.add_name, &i.add_pin, &i.add_pin2, &i.ed_name, &i.ed_new, &i.ed_new2, &i.up_new] {
            subs.push(cx.subscribe_in(l, window, |_, _, ev: &w::InputEvent, _, cx| if let w::InputEvent::Change = ev { cx.notify(); }));
        }
        for a in [&i.add_desc, &i.add_value, &i.ed_desc, &i.ed_value] {
            subs.push(cx.subscribe_in(a, window, |_, _, ev: &w::InputEvent, _, cx| if let w::InputEvent::Change = ev { cx.notify(); }));
        }
        // The PIN modal's field: a PIN takes 6 digits; Enter submits.
        subs.push(cx.subscribe_in(&i.pin, window, |v, st, ev: &w::InputEvent, window, cx| match ev {
            w::InputEvent::Change => {
                let lock = v.pin.as_ref().map(|p| p.lock.clone()).unwrap_or_default();
                keep_typed(st, &lock, window, cx);
                cx.notify();
            }
            w::InputEvent::PressEnter { .. } => window.defer(cx, pin_submit),
            _ => {}
        }));
        // Edit's current lock: the same rule.
        subs.push(cx.subscribe_in(&i.ed_cur, window, |v, st, ev: &w::InputEvent, window, cx| if let w::InputEvent::Change = ev {
            let lock = v.edit.as_ref().map(|e| e.job.lock.clone()).unwrap_or_default();
            keep_typed(st, &lock, window, cx);
            cx.notify();
        }));
        // The lock update's confirmation: Enter updates.
        subs.push(cx.subscribe_in(&i.up_new2, window, |_, _, ev: &w::InputEvent, window, cx| match ev {
            w::InputEvent::Change => cx.notify(),
            w::InputEvent::PressEnter { .. } => window.defer(cx, up_go),
            _ => {}
        }));
        Inputs { _subs: subs, ..i }
    }
}

/// A lock field keeps only what its lock takes (a PIN: 6 digits).
fn keep_typed(st: &Line, lock: &str, window: &mut Window, cx: &mut App) {
    let t = st.read(cx).value().to_string();
    let c = crundi_core::secrets::typed(lock, &t);
    if c != t { st.update(cx, |s, cx| s.set_value(c, window, cx)); }
}

struct Handle(Entity<Vault>);
impl Global for Handle {}

fn handle(cx: &mut App) -> Entity<Vault> {
    if let Some(h) = cx.try_global::<Handle>() { return h.0.clone(); }
    let e = cx.new(Vault::new);
    cx.set_global(Handle(e.clone()));
    e
}

/// The Secrets tab's badge in the shell's project bar (the requests waiting).
pub(crate) fn register_badge(cx: &mut App) {
    let e = handle(cx);
    crundi_shell::register_tab_badge(cx, "secrets", &e, |_, cx| { let b = badge(cx); (!b.is_empty()).then(|| w::tab_count(b).into_any_element()) });
}

/// The screen with its text boxes made (a window is at hand).
fn ready(window: &mut Window, cx: &mut App) -> Entity<Vault> {
    let e = handle(cx);
    if e.read(cx).inputs.is_none() {
        e.update(cx, |v, cx| { let i = Inputs::new(window, cx); v.inputs = Some(i); });
    }
    e
}

fn with<R>(cx: &mut App, f: impl FnOnce(&mut Vault, &mut Context<Vault>) -> R) -> R {
    let e = handle(cx);
    e.update(cx, |v, cx| { let r = f(v, cx); cx.notify(); r })
}

/// Work that needs the window, after the current update (a server's answer has none).
fn in_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    cx.defer(move |cx| {
        if let Some(h) = cx.windows().first().copied() { let _ = cx.update_window(h, |_, window, cx| f(window, cx)); }
    });
}

fn toast(cx: &mut App, text: impl Into<String>, kind: w::Toast) {
    let text = text.into();
    in_window(cx, move |window, cx| w::toast(window, cx, text, kind));
}

/// Set some of the boxes (picked from the inputs) to text.
fn set_lines(window: &mut Window, cx: &mut App, pick: impl Fn(&Inputs) -> Vec<(Line, String)>) {
    let e = ready(window, cx);
    let todo = pick(e.read(cx).inputs.as_ref().unwrap());
    for (l, t) in todo { l.update(cx, |s, cx| s.set_value(t, window, cx)); }
}
fn set_areas(window: &mut Window, cx: &mut App, pick: impl Fn(&Inputs) -> Vec<(Area, String)>) {
    let e = ready(window, cx);
    let todo = pick(e.read(cx).inputs.as_ref().unwrap());
    for (a, t) in todo { a.update(cx, |s, cx| s.set_value(t, window, cx)); }
}

/// Focus a box (and select its text: a refusal).
fn focus_line(window: &mut Window, cx: &mut App, select: bool, pick: impl Fn(&Inputs) -> Line) {
    let e = ready(window, cx);
    let l = pick(e.read(cx).inputs.as_ref().unwrap());
    l.update(cx, |s, cx| { s.focus(window, cx); if select { s.select_all(window, cx); } });
}

// ─── The spine's hooks ───

pub fn view(tab: &str, window: &mut Window, cx: &mut App) -> Option<AnyView> {
    if tab != TAB { return None; }
    Some(ready(window, cx).into())
}

pub fn on_event(e: &AreaEvent, cx: &mut App) {
    match e {
        AreaEvent::Server("secret-requests", v) => {
            let list = v.get("requests").and_then(|r| r.as_array()).cloned().unwrap_or_default();
            with(cx, |s, _| s.requests = list);
        }
        AreaEvent::Connected(true) => load(cx),
        _ => {}
    }
}

pub fn on_project(_alias: &str, _cx: &mut App) {}

pub fn on_signed_in(cx: &mut App) {
    with(cx, |s, _| { s.caps = None; s.loaded = false; s.error.clear(); });
    load(cx);
}

pub fn on_signed_out(cx: &mut App) {
    if !cx.has_global::<Handle>() { return; }
    with(cx, |s, _| {
        s.secrets.clear(); s.requests.clear(); s.revealed.clear(); s.loaded = false; s.error.clear();
        s.pin = None; s.edit = None; s.up = None; s.queue.clear(); s.caps = None; s.caps_wait.clear(); s.add_busy = false; s.copied.clear();
    });
    clear_add(cx);
    clear_locks(cx);
}

pub fn on_tab(tab: &str, cx: &mut App) {
    let was = with(cx, |s, _| std::mem::replace(&mut s.on_tab, tab == TAB));
    if tab == TAB { load(cx); }
    // Leaving the Secrets tab: forget every revealed value.
    else if was { with(cx, |s, _| s.revealed.clear()); }
}

/// The Secrets tab's badge: the requests waiting ("" for none).
pub fn badge(cx: &App) -> String { cx.try_global::<Handle>().map(|h| h.0.read(cx).badge()).unwrap_or_default() }

// ─── Loading ───

fn load(cx: &mut App) {
    if crundi_shell::api(cx).is_none() { return; }
    crundi_shell::call(cx, |api| api.get("/api/secrets"), |r, cx| with(cx, |s, _| match r {
        Ok(v) => {
            if ok(&v) {
                s.secrets = v.get("secrets").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                s.requests = v.get("requests").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                // A value revealed for a secret that is gone goes with it.
                let ids: Vec<String> = s.secrets.iter().map(|x| rules::s(x, "id")).collect();
                s.revealed.retain(|k, _| ids.contains(k));
            }
            s.loaded = true;
            s.error.clear();
        }
        Err(e) => s.error = e,
    }));
}

/// Does the server list this capability? (Asked once per sign-in.)
fn supports(cx: &mut App, cap: &str, then: impl FnOnce(bool, &mut App) + 'static) {
    let known = handle(cx).read(cx).caps.as_ref().map(|c| c.iter().any(|x| x == cap));
    if let Some(k) = known { then(k, cx); return; }
    let first = with(cx, |s, _| { s.caps_wait.push((cap.to_string(), Box::new(then))); s.caps_wait.len() == 1 });
    if !first { return; }
    let Some(base) = crundi_shell::api(cx).map(|a| a.base().to_string()) else { return };
    crundi_shell::run(cx, move || crundi_core::api::handshake(&base).ok().and_then(|h| h.pointer("/server/capabilities").cloned()), |caps, cx| {
        let list: Vec<String> = caps.and_then(|c| c.as_array().cloned()).unwrap_or_default().iter().filter_map(|x| x.as_str().map(String::from)).collect();
        let waiting = with(cx, |s, _| { s.caps = Some(list.clone()); std::mem::take(&mut s.caps_wait) });
        for (cap, f) in waiting { f(list.contains(&cap), cx); }
    });
}

fn post(cx: &mut App, body: Value, done: impl FnOnce(Value, &mut App) + 'static) {
    if crundi_shell::api(cx).is_none() { done(json!({ "ok": false, "error": "Not ready" }), cx); return; }
    crundi_shell::call(cx, move |api| api.post("/api/secrets", &body), move |r, cx| done(rules::answer(r), cx));
}

// ─── Add a secret ───

fn clear_add(cx: &mut App) {
    in_window(cx, |window, cx| {
        set_lines(window, cx, |i| vec![(i.add_name.clone(), String::new()), (i.add_pin.clone(), String::new()), (i.add_pin2.clone(), String::new())]);
        set_areas(window, cx, |i| vec![(i.add_desc.clone(), String::new()), (i.add_value.clone(), String::new())]);
    });
}

fn add_save(window: &mut Window, cx: &mut App) {
    let e = ready(window, cx);
    if e.read(cx).add_busy { return; }
    let (name, desc, value, pin, pin2) = add_vals(e.read(cx).inputs.as_ref().unwrap(), cx);
    let name = name.trim().to_string();
    if name.is_empty() { w::toast(window, cx, "Name is required", w::Toast::Error); return; }
    if value.is_empty() { w::toast(window, cx, "Value is required", w::Toast::Error); return; }
    if !passphrase_ok(&pin) { w::toast(window, cx, "Passphrase: at least 8 characters, and not only digits", w::Toast::Error); return; }
    if pin != pin2 { w::toast(window, cx, "The two passphrases do not match", w::Toast::Error); return; }
    with(cx, |s, _| s.add_busy = true);
    post(cx, json!({ "action": "add", "name": name, "description": desc, "value": value, "pin": pin }), |d, cx| {
        with(cx, |s, _| s.add_busy = false);
        if ok(&d) {
            toast(cx, "Secret saved", w::Toast::Success);
            // The web draws the tab again: the form comes back empty.
            clear_add(cx);
            load(cx);
        } else { toast(cx, err_or(&d, "Failed to save"), w::Toast::Error); }
    });
}

// ─── The list ───

fn reveal(id: &str, window: &mut Window, cx: &mut App) {
    let Some(sec) = handle(cx).read(cx).secret(id) else { return };
    let (lock, name) = (lock_of(&sec), s(&sec, "name"));
    pin_open(PinJob::Reveal { id: id.into(), name: name.clone() }, lock, "Reveal secret", format!("Enter the {} for \"{name}\".", ask_word(lock)), "Reveal", window, cx);
}

fn hide(id: &str, cx: &mut App) { let id = id.to_string(); with(cx, |s, _| { s.revealed.remove(&id); }); }

fn copy(id: &str, window: &mut Window, cx: &mut App) {
    let Some(text) = handle(cx).read(cx).revealed.get(id).cloned() else { return };
    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
    with(cx, |s, _| s.copied = text);
    w::toast(window, cx, "Copied", w::Toast::Success);
}

fn delete(id: &str, window: &mut Window, cx: &mut App) {
    let Some(sec) = handle(cx).read(cx).secret(id) else { return };
    let name = s(&sec, "name");
    let id = id.to_string();
    w::ask_confirm(window, cx, w::Ask { title: "Delete secret".into(), message: format!("Delete secret \"{name}\"? This cannot be undone.").into(), ok: "Delete".into(), danger: true, ..Default::default() }, move |yes, _, cx| {
        if !yes { return; }
        let id = id.clone();
        post(cx, json!({ "action": "delete", "id": id }), move |d, cx| {
            if ok(&d) { with(cx, |s, _| { s.revealed.remove(&id); }); toast(cx, "Deleted", w::Toast::Success); load(cx); }
            else { toast(cx, err_or(&d, "Failed to delete"), w::Toast::Error); }
        });
    });
}

fn approve(req_id: &str, window: &mut Window, cx: &mut App) {
    let Some(r) = handle(cx).read(cx).requests.iter().find(|x| s(x, "id") == req_id).cloned() else { return };
    let lock = lock_of(&r);
    let name = s(&r, "secretName");
    let job = PinJob::Approve { req: req_id.into(), secret: s(&r, "secretId"), name: name.clone(), upgrade: truthy(&r, "needsUpgrade") };
    pin_open(job, lock, "Approve access", format!("Enter the {} for \"{name}\" to release it to Claude.", ask_word(lock)), "Approve", window, cx);
}

fn deny(id: &str, cx: &mut App) {
    post(cx, json!({ "action": "deny", "reqId": id }), |_, cx| load(cx));
}

// ─── The PIN modal ───

fn pin_open(job: PinJob, lock: &str, title: &str, sub: String, label: &str, window: &mut Window, cx: &mut App) {
    let e = ready(window, cx);
    let pin = e.read(cx).inputs.as_ref().unwrap().pin.clone();
    let ph = if lock == "passphrase" { "Passphrase" } else { "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}" };
    pin.update(cx, |s, cx| { s.set_value("", window, cx); s.set_placeholder(ph, window, cx); });
    with(cx, |s, _| s.pin = Some(Pin { job, lock: lock.into(), title: title.into(), sub, label: label.into(), error: String::new(), busy: false }));
    focus_line(window, cx, false, |i| i.pin.clone());
}

fn pin_close(cx: &mut App) {
    with(cx, |s, _| s.pin = None);
    in_window(cx, |window, cx| set_lines(window, cx, |i| vec![(i.pin.clone(), String::new())]));
}

fn pin_submit(window: &mut Window, cx: &mut App) {
    let e = ready(window, cx);
    let Some((lock, body)) = ({
        let v = e.read(cx);
        v.pin.as_ref().filter(|p| !p.busy).map(|p| {
            let raw = lv(&v.inputs.as_ref().unwrap().pin, cx);
            let pin = if p.lock == "passphrase" { raw } else { raw.trim().to_string() };
            (p.lock.clone(), match &p.job {
                PinJob::Reveal { id, .. } => json!({ "action": "reveal", "id": id, "pin": pin }),
                PinJob::Approve { req, .. } => json!({ "action": "approve", "reqId": req, "pin": pin }),
            })
        })
    }) else { return };
    let pin = s(&body, "pin");
    if let Err(m) = crundi_core::sreq::check(&lock, &pin) { with(cx, |s, _| if let Some(p) = &mut s.pin { p.error = m.into(); }); return; }
    with(cx, |s, _| if let Some(p) = &mut s.pin { p.busy = true; });
    post(cx, body, move |d, cx| {
        if !ok(&d) {
            // A refusal keeps the modal; the field selects its text.
            with(cx, |s, _| if let Some(p) = &mut s.pin { p.busy = false; p.error = err_or(&d, "Failed"); });
            in_window(cx, |window, cx| focus_line(window, cx, true, |i| i.pin.clone()));
            return;
        }
        let job = with(cx, |s, _| s.pin.take()).map(|p| p.job);
        pin_close(cx);
        let needs = truthy(&d, "needsUpgrade");
        match job {
            Some(PinJob::Reveal { id, name }) => {
                let value = s(&d, "value");
                with(cx, |s, _| { s.revealed.insert(id.clone(), value); });
                if needs { upgrade(&id, &name, &lock, &pin, cx); }
            }
            Some(PinJob::Approve { secret, name, upgrade: old, .. }) => {
                toast(cx, "Approved", w::Toast::Success);
                load(cx);
                if needs || old { upgrade(&secret, &name, &lock, &pin, cx); }
            }
            None => {}
        }
    });
}

// ─── Edit secret ───

fn edit(id: &str, cx: &mut App) {
    let Some(sec) = handle(cx).read(cx).secret(id) else { return };
    supports(cx, "secret-edit", move |yes, cx| {
        if !yes { toast(cx, UNSUPPORTED, w::Toast::Error); return; }
        in_window(cx, move |window, cx| {
            let job = EditJob { id: s(&sec, "id"), name: s(&sec, "name"), desc: s(&sec, "description"), lock: lock_of(&sec).into() };
            let ph = if job.lock == "passphrase" { "Passphrase" } else { "PIN" };
            let (n, d) = (job.name.clone(), job.desc.clone());
            set_lines(window, cx, |i| vec![(i.ed_name.clone(), n.clone()), (i.ed_new.clone(), String::new()), (i.ed_new2.clone(), String::new()), (i.ed_cur.clone(), String::new())]);
            set_areas(window, cx, |i| vec![(i.ed_desc.clone(), d.clone()), (i.ed_value.clone(), String::new())]);
            let cur = ready(window, cx).read(cx).inputs.as_ref().unwrap().ed_cur.clone();
            cur.update(cx, |s, cx| s.set_placeholder(ph, window, cx));
            with(cx, |s, _| s.edit = Some(Edit { job, relock: false, err: String::new(), busy: false }));
            focus_line(window, cx, false, |i| i.ed_name.clone());
        });
    });
}

fn ed_relock(on: bool, cx: &mut App) {
    with(cx, |s, _| if let Some(e) = &mut s.edit { e.relock = on; });
    if !on { in_window(cx, |window, cx| set_lines(window, cx, |i| vec![(i.ed_new.clone(), String::new()), (i.ed_new2.clone(), String::new())])); }
    else { in_window(cx, |window, cx| focus_line(window, cx, false, |i| i.ed_new.clone())); }
}

fn ed_close(cx: &mut App) {
    let had = with(cx, |s, _| s.edit.take().is_some());
    in_window(cx, |window, cx| {
        set_lines(window, cx, |i| vec![(i.ed_cur.clone(), String::new()), (i.ed_new.clone(), String::new()), (i.ed_new2.clone(), String::new())]);
        set_areas(window, cx, |i| vec![(i.ed_value.clone(), String::new())]);
    });
    if had { next_up(cx); }
}

fn ed_save(cx: &mut App) {
    let e = handle(cx);
    if e.read(cx).edit.as_ref().is_none_or(|x| x.busy) { return; }
    let Some(Some(body)) = ed_form(e.read(cx), cx, |f, id| f.ok().then(|| f.body(id))) else { return };
    let id = s(&body, "id");
    with(cx, |s, _| if let Some(e) = &mut s.edit { e.busy = true; e.err.clear(); });
    post(cx, body, move |d, cx| {
        if handle(cx).read(cx).edit.as_ref().is_none_or(|e| e.job.id != id) { return; }
        if ok(&d) {
            toast(cx, "Secret saved", w::Toast::Success);
            with(cx, |s, _| { s.revealed.remove(&id); });
            ed_close(cx);
            load(cx);
            return;
        }
        let err = err_or(&d, "Could not save it.");
        let again = rules::lockish(&err);
        with(cx, |s, _| if let Some(e) = &mut s.edit { e.busy = false; e.err = err; });
        if again { in_window(cx, |window, cx| focus_line(window, cx, true, |i| i.ed_cur.clone())); }
    });
}

// ─── Update this secret's lock ───

/// After an old lock was accepted: offer to set a passphrase now, with the
/// lock just typed (kept in this offer only). Asked only when the server
/// lists `secret-upgrade`; one dialog at a time (the others wait).
pub fn upgrade(id: &str, name: &str, lock: &str, pin: &str, cx: &mut App) {
    if id.is_empty() || pin.is_empty() { return; }
    let job = UpJob { id: id.into(), name: name.into(), lock: lock.into(), pin: pin.into() };
    supports(cx, "secret-upgrade", move |yes, cx| {
        if !yes { return; }
        let busy = handle(cx).read(cx).up.is_some() || handle(cx).read(cx).edit.is_some();
        if busy { with(cx, |s, _| s.queue.push_back(job)); } else { up_show(job, cx); }
    });
}

fn up_show(job: UpJob, cx: &mut App) {
    with(cx, |s, _| s.up = Some(Up { job, err: String::new(), busy: false }));
    in_window(cx, |window, cx| {
        set_lines(window, cx, |i| vec![(i.up_new.clone(), String::new()), (i.up_new2.clone(), String::new())]);
        focus_line(window, cx, false, |i| i.up_new.clone());
    });
}

fn up_close(cx: &mut App) {
    let had = with(cx, |s, _| s.up.take().is_some());
    in_window(cx, |window, cx| set_lines(window, cx, |i| vec![(i.up_new.clone(), String::new()), (i.up_new2.clone(), String::new())]));
    if had { next_up(cx); }
}

fn next_up(cx: &mut App) {
    let next = with(cx, |s, _| if s.up.is_none() && s.edit.is_none() { s.queue.pop_front() } else { None });
    if let Some(j) = next { up_show(j, cx); }
}

fn up_vals(v: &Vault, cx: &App) -> (String, String) {
    v.inputs.as_ref().map(|i| (lv(&i.up_new, cx), lv(&i.up_new2, cx))).unwrap_or_default()
}

fn up_go(_window: &mut Window, cx: &mut App) {
    let e = handle(cx);
    let (a, b) = up_vals(e.read(cx), cx);
    let Some(job) = e.read(cx).up.as_ref().filter(|u| !u.busy).map(|u| u.job.clone()) else { return };
    if !(passphrase_ok(&a) && a == b) { return; }
    with(cx, |s, _| if let Some(u) = &mut s.up { u.busy = true; u.err.clear(); });
    let id = job.id.clone();
    post(cx, json!({ "action": "edit", "id": job.id, "pin": job.pin, "newPin": a }), move |d, cx| {
        if handle(cx).read(cx).up.as_ref().is_none_or(|u| u.job.id != id) { return; }
        if ok(&d) {
            toast(cx, "Lock updated", w::Toast::Success);
            up_close(cx);
            load(cx);
            return;
        }
        with(cx, |s, _| if let Some(u) = &mut s.up { u.busy = false; u.err = err_or(&d, "Could not update it."); });
    });
}

/// Every lock box emptied (signing out).
fn clear_locks(cx: &mut App) {
    in_window(cx, |window, cx| set_lines(window, cx, |i| vec![
        (i.pin.clone(), String::new()), (i.ed_cur.clone(), String::new()), (i.ed_new.clone(), String::new()), (i.ed_new2.clone(), String::new()),
        (i.up_new.clone(), String::new()), (i.up_new2.clone(), String::new()),
    ]));
}

// ─── Drawing ───

/// A section's title (.secrets-section-title).
fn sec_title(text: impl Into<SharedString>) -> Div {
    div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(text.into())
}

/// A dialog's small upper-case label (.csl-sec).
fn sec_label(text: &'static str) -> Div {
    div().text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(text)
}

/// A hint line (.csl-hint): yellow when it warns.
fn hint(text: impl Into<SharedString>, warn: bool) -> Div {
    div().text_size(T::TEXT_12).text_color(if warn { T::yellow() } else { T::text_secondary() }).child(text.into())
}

/// A lock dialog's head (.csl-head): the lock in accent, the title.
fn lock_head(title: &'static str) -> Div {
    div().flex().items_center().gap(T::SP_8)
        .child(div().text_color(T::accent_hover()).child(w::icon(IconName::Lock, T::ICON_MD)))
        .child(div().flex_1().min_w_0().text_size(T::TEXT_15).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(title))
}

/// A lock field: a PIN mono and centred, a passphrase plain (both hidden).
/// In the PIN dialog (`big`, .pin-modal input) the text is centred: a PIN
/// mono at 1.4rem, a passphrase (.pass) in the plain face at 0.95rem; the
/// placeholder takes the same size.
fn lock_field(l: &Line, lock: &str, big: bool) -> Div {
    let pin = lock != "passphrase";
    let i = w::input(l).when(pin, |d| d.font_family(T::MONO))
        .when(big, |d| d.text_align(TextAlign::Center).text_size(if pin { T::VAULT_PIN_TEXT } else { T::TEXT_15 }).when(pin, |d| d.h(T::INPUT_H + T::SP_12)));
    div().w_full().child(i)
}

/// A multi-line field with the inputs' text size and padding (the web's
/// `.secrets-add textarea`: 0.85rem, 8px 10px, like its inputs).
fn field_area(state: &Entity<w::TextareaState>) -> w::Textarea {
    w::textarea(state).text_size(T::INPUT_TEXT).px(T::INPUT_PAD_X).py(T::SP_8)
}

fn error_line(text: &str) -> Option<Div> {
    (!text.is_empty()).then(|| div().text_size(T::TEXT_12).text_color(T::red()).child(text.to_string()))
}

impl Vault {
    fn requests_box(&self) -> Div {
        let mut d = div().flex().flex_col().gap(T::SP_8).p(T::SP_12).rounded(T::R_CARD).bg(T::yellow_dim()).border_1().border_color(T::yellow())
            .child(sec_title("PENDING ACCESS REQUESTS"));
        for (i, r) in self.requests.iter().enumerate() {
            let (id1, id2) = (s(r, "id"), s(r, "id"));
            d = d.child(div().flex().items_center().gap(T::SP_10).when(i > 0, |d| d.pt(T::SP_8).border_t_1().border_color(T::vault_yellow_edge()))
                .child(div().flex().flex_col().flex_1().min_w_0().gap(T::SP_2)
                    .child(div().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(s(r, "secretName")))
                    .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(req_meta(r))))
                .child(w::primary(("vault-approve", i), "Approve").on_click(move |_, window, cx| approve(&id1, window, cx)))
                .child(w::button(("vault-deny", i), "Deny").on_click(move |_, _, cx| deny(&id2, cx))));
        }
        d
    }

    fn add_card(&self, cx: &App) -> Div {
        let i = self.inputs.as_ref().unwrap();
        let (name, _, value, a, b) = add_vals(i, cx);
        let (rule, warn) = new_lock_hint(&a, &b);
        let can = add_ok(&name, &value, &a, &b) && !self.add_busy;
        div().flex().flex_col().gap(T::SP_8).p(T::SP_16).rounded(T::R_CARD).bg(T::bg_secondary()).border_1().border_color(T::border())
            .child(sec_title("ADD A SECRET"))
            .child(w::input(&i.add_name))
            .child(field_area(&i.add_desc))
            .child(field_area(&i.add_value))
            .child(w::input(&i.add_pin))
            .child(w::input(&i.add_pin2))
            .child(div().text_size(T::TEXT_12).text_color(if warn { T::yellow() } else { T::text_muted() }).child(rule))
            .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("The passphrase is never stored. If you lose it, this secret can only be deleted & recreated."))
            .child(div().flex().child(w::disabled(w::primary("vault-add-save", "Save secret"), !can).on_click(|_, window, cx| add_save(window, cx))))
    }

    fn list(&self) -> Div {
        let mut d = div().flex().flex_col().gap(T::SP_8).child(sec_title(format!("SECRETS ({})", self.secrets.len())));
        if self.secrets.is_empty() {
            return d.child(div().flex().justify_center().py(T::SP_24).text_size(T::TEXT_13).text_color(T::text_muted()).child("No secrets yet."));
        }
        for (n, x) in self.secrets.iter().enumerate() {
            let id = s(x, "id");
            let (tag, tip) = tag_of(x);
            let desc = s(x, "description");
            let val = self.revealed.get(&id).cloned();
            let revealed = val.is_some();
            let (a, b, c, e) = (id.clone(), id.clone(), id.clone(), id.clone());
            let mut actions = div().flex().flex_none().items_center().gap(T::SP_6);
            if revealed {
                actions = actions
                    .child(w::button(("vault-copy", n), "Copy").on_click(move |_, window, cx| copy(&a, window, cx)))
                    .child(w::button(("vault-hide", n), "Hide").on_click(move |_, _, cx| hide(&b, cx)));
            } else {
                actions = actions.child(w::button(("vault-reveal", n), "Reveal").on_click(move |_, window, cx| reveal(&a, window, cx)));
            }
            actions = actions
                .child(w::button(("vault-edit", n), "Edit").on_click(move |_, _, cx| edit(&c, cx)))
                .child(w::danger(("vault-delete", n), "Delete").on_click(move |_, window, cx| delete(&e, window, cx)));
            d = d.child(div().id(("vault-row", n)).flex().items_center().gap(T::SP_10).px(T::SP_12).py(T::SP_10)
                .rounded(T::R_CONTROL).bg(T::bg_card()).border_1().border_color(T::border()).hover(|d| d.bg(T::bg_hover()))
                .child(div().flex().flex_col().flex_1().min_w_0().gap(T::SP_2)
                    .child(div().flex().items_center().gap(T::SP_6)
                        .child(div().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(s(x, "name")))
                        .when(!tag.is_empty(), |d| d.child(w::tip(div().id(("vault-tag", n)).child(w::tag(tag, "warn")), tip))))
                    .when(!desc.is_empty(), |d| d.child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(desc)))
                    .children(val.map(|v| div().text_size(T::TEXT_12).font_family(T::MONO).text_color(T::green()).child(v))))
                .child(actions));
        }
        d
    }

    /// The open dialog over the window (one at a time: PIN, Edit, the lock update).
    fn dialog(&self, window: &mut Window, cx: &App) -> Option<AnyElement> {
        let i = self.inputs.as_ref()?;
        let (width, children, esc): (Pixels, Vec<AnyElement>, fn(&mut App)) = if let Some(p) = &self.pin {
            let label = p.label.clone();
            let kids = vec![div().flex().flex_col().gap(T::SP_6).p(T::SP_16)
                .child(div().flex().justify_center().text_size(T::TEXT_16).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(p.title.clone()))
                .child(div().flex().justify_center().text_size(T::TEXT_13).text_color(T::text_secondary()).child(p.sub.clone()))
                .child(div().h(T::SP_6))
                .child(lock_field(&i.pin, &p.lock, true))
                .child(div().flex().justify_center().min_h(T::SP_16).text_size(T::TEXT_12).text_color(T::red()).child(p.error.clone()))
                .child(div().flex().gap(T::SP_10)
                    .child(div().flex_1().min_w_0().child(w::button("vault-pin-cancel", "Cancel").w_full().on_click(|_, _, cx| pin_close(cx))))
                    .child(div().flex_1().min_w_0().child(w::disabled(w::primary("vault-pin-ok", label), p.busy).w_full().on_click(|_, window, cx| pin_submit(window, cx)))))
                .into_any_element()];
            (T::VAULT_PIN_W, kids, pin_close)
        } else if let Some(e) = &self.edit {
            let (okd, (h, warn)) = ed_form(self, cx, |f, _| (f.ok(), new_lock_hint(f.new, f.new2))).unwrap_or((false, (String::new(), false)));
            let pass = e.job.lock == "passphrase";
            let relock = e.relock;
            let kids = vec![div().flex().flex_col().gap(T::SP_8).p(T::SP_12)
                .child(lock_head("Edit secret"))
                .child(sec_label("NAME AND DESCRIPTION"))
                .child(w::input(&i.ed_name))
                .child(field_area(&i.ed_desc))
                .child(sec_label("NEW VALUE"))
                .child(div().font_family(T::MONO).child(field_area(&i.ed_value)))
                .child(sec_label("NEW PASSPHRASE"))
                .child(w::check("vault-ed-relock", if pass { "Change the passphrase" } else { "Replace the 6-digit PIN with a passphrase" }, relock, |on, _, cx| ed_relock(*on, cx)))
                .when(relock, |d| d.child(lock_field(&i.ed_new, "passphrase", false)).child(lock_field(&i.ed_new2, "passphrase", false)).child(hint(h, warn)))
                .child(sec_label(if pass { "CURRENT PASSPHRASE" } else { "CURRENT PIN" }))
                .child(lock_field(&i.ed_cur, &e.job.lock, false))
                .child(hint("Needed for any change to the value or the lock. It is never stored.", false))
                .children(error_line(&e.err))
                .child(div().flex().justify_end().gap(T::SP_8).pt(T::SP_4)
                    .child(w::button("vault-ed-cancel", "Cancel").on_click(|_, _, cx| ed_close(cx)))
                    .child(w::disabled(w::primary("vault-ed-save", if e.busy { "Saving\u{2026}" } else { "Save" }), !okd || e.busy).on_click(|_, _, cx| ed_save(cx))))
                .into_any_element()];
            (T::VAULT_LOCK_W, kids, ed_close)
        } else if let Some(u) = &self.up {
            let (a, b) = up_vals(self, cx);
            let (h, warn) = new_lock_hint(&a, &b);
            let okd = passphrase_ok(&a) && a == b;
            let name = if u.job.name.is_empty() { "This secret".to_string() } else { u.job.name.clone() };
            let old = u.job.lock != "passphrase";
            let kids = vec![div().flex().flex_col().gap(T::SP_8).p(T::SP_12)
                .child(lock_head("Update this secret\u{2019}s lock"))
                .child(div().font_family(T::MONO).text_size(T::TEXT_13).text_color(T::accent_hover()).child(name))
                .child(div().text_size(T::TEXT_13).text_color(T::text()).child(if old {
                    "This secret uses an old 6-digit PIN. Set a new passphrase (8+ characters)."
                } else {
                    "This secret uses an older, weaker lock. Set a passphrase (8+ characters) to seal it again; it may be the same one."
                }))
                .child(lock_field(&i.up_new, "passphrase", false))
                .child(lock_field(&i.up_new2, "passphrase", false))
                .child(hint(h, warn))
                .children(error_line(&u.err))
                .child(div().flex().justify_end().gap(T::SP_8).pt(T::SP_4)
                    .child(w::disabled(w::button("vault-up-later", "Not now"), u.busy).on_click(|_, _, cx| up_close(cx)))
                    .child(w::disabled(w::primary("vault-up-go", if u.busy { "Updating\u{2026}" } else { "Update" }), !okd || u.busy).on_click(|_, window, cx| up_go(window, cx))))
                .into_any_element()];
            (T::VAULT_LOCK_W, kids, up_close)
        } else { return None };
        let m = w::Modal {
            id: "vault-dlg", width, focus: self.focus.clone(), scroll: self.scroll.clone(),
            // The scrim does nothing (as the web's); Esc closes.
            on_close: Rc::new(|_, _| {}),
            on_key: Rc::new(move |e, _, cx| if e.keystroke.key == "escape" { esc(cx) }),
        };
        Some(w::overlay_modal(m, window, children).into_any_element())
    }
}

impl Render for Vault {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("vault");
        let mut page = w::page("vault-page");
        if !self.error.is_empty() {
            page = page.child(w::error_state("vault-retry", format!("Failed to load: {}", self.error), |_, _, cx| load(cx)));
        } else if !self.loaded {
            page = page.child(w::loading("Loading\u{2026}"));
        } else {
            page = page
                .when(!self.requests.is_empty(), |d| d.child(self.requests_box()))
                .child(self.add_card(cx))
                .child(self.list());
        }
        div().size_full().child(page).children(self.dialog(window, cx))
    }
}

// ─── The test driver ───

/// The driver's "ops:<name>" actions this screen knows (None: not mine).
pub fn test_ui(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    if !name.starts_with("vault-") { return None; }
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let e = ready(window, cx);
    let id_of = |n: &str, cx: &App| e.read(cx).by_name(n);
    match name {
        "vault-add-set" => {
            let t = a(1);
            match a(0).as_str() {
                "name" => set_lines(window, cx, |i| vec![(i.add_name.clone(), t.clone())]),
                "pin" => set_lines(window, cx, |i| vec![(i.add_pin.clone(), t.clone())]),
                "pin2" => set_lines(window, cx, |i| vec![(i.add_pin2.clone(), t.clone())]),
                "desc" => set_areas(window, cx, |i| vec![(i.add_desc.clone(), t.clone())]),
                "value" => set_areas(window, cx, |i| vec![(i.add_value.clone(), t.clone())]),
                _ => return Some(json!({ "ok": false })),
            }
        }
        "vault-add-save" => add_save(window, cx),
        "vault-reveal" => { let id = id_of(&a(0), cx); reveal(&id, window, cx) }
        "vault-hide" => { let id = id_of(&a(0), cx); hide(&id, cx) }
        "vault-copy" => { let id = id_of(&a(0), cx); copy(&id, window, cx) }
        "vault-clipboard" => {
            let t = cx.read_from_clipboard().and_then(|c| c.text()).unwrap_or_else(|| e.read(cx).copied.clone());
            return Some(json!({ "ok": true, "text": t }));
        }
        "vault-delete" => { let id = id_of(&a(0), cx); delete(&id, window, cx) }
        "vault-edit" => { let id = id_of(&a(0), cx); edit(&id, cx) }
        "vault-approve" => { let id = e.read(cx).req_by_name(&a(0)); approve(&id, window, cx) }
        "vault-deny" => { let id = e.read(cx).req_by_name(&a(0)); deny(&id, cx) }
        "vault-pin-type" => {
            let lock = e.read(cx).pin.as_ref().map(|p| p.lock.clone()).unwrap_or_default();
            let t = crundi_core::secrets::typed(&lock, &a(0));
            set_lines(window, cx, |i| vec![(i.pin.clone(), t.clone())]);
        }
        "vault-pin-submit" => pin_submit(window, cx),
        "vault-pin-cancel" => pin_close(cx),
        "vault-edit-set" => {
            let t = a(1);
            match a(0).as_str() {
                "name" => set_lines(window, cx, |i| vec![(i.ed_name.clone(), t.clone())]),
                "new" => set_lines(window, cx, |i| vec![(i.ed_new.clone(), t.clone())]),
                "new2" => set_lines(window, cx, |i| vec![(i.ed_new2.clone(), t.clone())]),
                "cur" => {
                    let lock = e.read(cx).edit.as_ref().map(|x| x.job.lock.clone()).unwrap_or_default();
                    let c = crundi_core::secrets::typed(&lock, &t);
                    set_lines(window, cx, |i| vec![(i.ed_cur.clone(), c.clone())]);
                }
                "desc" => set_areas(window, cx, |i| vec![(i.ed_desc.clone(), t.clone())]),
                "value" => set_areas(window, cx, |i| vec![(i.ed_value.clone(), t.clone())]),
                _ => return Some(json!({ "ok": false })),
            }
        }
        "vault-edit-relock" => ed_relock(a(0) != "false", cx),
        "vault-edit-save" => ed_save(cx),
        "vault-edit-cancel" => ed_close(cx),
        "vault-up-offer" => { let job = UpJob { id: id_of(&a(0), cx), name: a(0), lock: a(1), pin: a(2) }; up_show(job, cx); }
        "vault-up-set" => { let t = a(1); if a(0) == "new" { set_lines(window, cx, |i| vec![(i.up_new.clone(), t.clone())]) } else { set_lines(window, cx, |i| vec![(i.up_new2.clone(), t.clone())]) } }
        "vault-up-go" => up_go(window, cx),
        "vault-up-later" => up_close(cx),
        _ => return None,
    }
    cx.notify(e.entity_id());
    Some(json!({ "ok": true }))
}

/// The driver's view of this screen (the Slint client's names). A value is
/// in it only while revealed.
pub fn props(window: &mut Window, cx: &mut App) -> Value {
    let e = ready(window, cx);
    let v = e.read(cx);
    let i = v.inputs.as_ref().unwrap();
    let secrets: Vec<Value> = v.secrets.iter().map(|x| {
        let id = s(x, "id");
        let val = v.revealed.get(&id);
        let mut o = json!({ "name": s(x, "name"), "desc": s(x, "description"), "tag": tag_of(x).0, "revealed": val.is_some() });
        if let Some(val) = val { o["value"] = json!(val); }
        o
    }).collect();
    let (name, _, value, a, b) = add_vals(i, cx);
    let (rule, warn) = new_lock_hint(&a, &b);
    let pin = v.pin.as_ref().map(|p| json!({ "title": p.title, "sub": p.sub, "label": p.label, "lock": p.lock, "error": p.error, "typed": lv(&i.pin, cx).chars().count(), "busy": p.busy })).unwrap_or(Value::Null);
    let edit = v.edit.as_ref().map(|e| {
        let (okd, (h, warn)) = ed_form(v, cx, |f, _| (f.ok(), new_lock_hint(f.new, f.new2))).unwrap_or((false, (String::new(), false)));
        json!({ "lock": e.job.lock, "name": lv(&i.ed_name, cx), "relock": e.relock, "hint": h, "warn": warn, "ok": okd, "err": e.err, "busy": e.busy })
    }).unwrap_or(Value::Null);
    let upgrade = v.up.as_ref().map(|u| {
        let (a, b) = up_vals(v, cx);
        let (h, warn) = new_lock_hint(&a, &b);
        json!({ "name": if u.job.name.is_empty() { "This secret".to_string() } else { u.job.name.clone() }, "oldPin": u.job.lock != "passphrase", "hint": h, "warn": warn, "ok": passphrase_ok(&a) && a == b, "err": u.err })
    }).unwrap_or(Value::Null);
    json!({
        "loaded": v.loaded, "error": v.error, "badge": v.badge(), "modal": v.modal(),
        "requests": v.requests.iter().map(|r| json!({ "name": s(r, "secretName"), "meta": req_meta(r) })).collect::<Vec<_>>(),
        "secrets": secrets,
        "add": { "rule": rule, "warn": warn, "ok": add_ok(&name, &value, &a, &b), "name": name },
        "pin": pin, "edit": edit, "upgrade": upgrade,
    })
}
