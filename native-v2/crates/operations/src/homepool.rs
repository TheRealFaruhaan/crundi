//! Homepool (the Homepool tab; Slint: src/ops/homepool.rs, ui/ops/homepool.slint,
//! specs/ops/homepool.md; the web's app/vendor/crundi-homepool.js and
//! loadHomepoolTab): networks and their nodes, access keys, the chats'
//! requests, the activity log, the summary, and the dialogs (add a node,
//! give or change access, a node's code and a key's secret shown once).
//! GET /api/homepool every 3 s while the tab is on show (not while a dialog
//! is open), POST /api/homepool {action} for changes. Confirms and prompts
//! are the shared ones (crundi_widgets::ask).
//!
//! The web's "This PC" card (the desktop app lending its connection) is the
//! desktop area's homepool node; it is not drawn here (PLAN.md).
//!
//! The screen is one entity in a Global (`Hp`); every click and driver
//! action is a free function here, which updates it through `with`.

mod rules;

use crate::compat::{session, AreaEvent};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use rules::{arr, home_nets, ss, t, use_line, Model, Use};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashSet, rc::Rc, time::Duration};

const STEM: &str = "homepool";

/// What the open dialog is for.
#[derive(Clone, Debug, Default, PartialEq)]
enum Dlg {
    #[default]
    None,
    Node,
    /// Give access (both None), change a key (edit), or approve a request with changes.
    Key { edit: Option<String>, request: Option<String> },
    Show,
}

/// The dialogs' text boxes (made once a window is at hand).
struct Inputs {
    n_name: Entity<w::InputState>,
    k_name: Entity<w::InputState>,
    k_chat: Entity<w::InputState>,
    k_exp: Entity<w::InputState>,
    k_cap: Entity<w::InputState>,
    k_slots: Entity<w::InputState>,
    k_from: Entity<w::InputState>,
}

pub struct Hp {
    shown: bool,
    data: Option<Value>,
    error: String,
    tab: String,
    open_key: String,
    dlg: Dlg,
    dlg_title: String,
    dlg_label: String,
    inputs: Option<Inputs>,
    // The node dialog: the network select's labels and ids.
    n_net: usize,
    n_nets: Vec<String>,
    n_net_ids: Vec<String>,
    // The access dialog.
    k_kind: usize,
    k_kind_locked: bool,
    k_project: usize,
    k_projects: Vec<String>,
    k_project_ids: Vec<String>,
    k_project_locked: bool,
    k_net_ids: Vec<String>,
    k_nets_on: Vec<bool>,
    k_fb: bool,
    k_block: bool,
    // The show dialog.
    show_warn: String,
    show_items: Vec<Use>,
    qr: Option<(Vec<(f32, f32, f32)>, f32)>,
    show_warn2: String,
    copied: String,
    poll: Option<Task<()>>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    page_scroll: ScrollHandle,
}

impl Hp {
    fn new(cx: &mut Context<Self>) -> Self {
        Hp {
            shown: false, data: None, error: String::new(), tab: "networks".into(), open_key: String::new(),
            dlg: Dlg::None, dlg_title: String::new(), dlg_label: String::new(), inputs: None,
            n_net: 0, n_nets: vec![], n_net_ids: vec![],
            k_kind: 0, k_kind_locked: false, k_project: 0, k_projects: vec![], k_project_ids: vec![], k_project_locked: false,
            k_net_ids: vec![], k_nets_on: vec![], k_fb: false, k_block: false,
            show_warn: String::new(), show_items: vec![], qr: None, show_warn2: String::new(), copied: String::new(),
            poll: None, focus: cx.focus_handle(), scroll: ScrollHandle::new(), page_scroll: ScrollHandle::new(),
        }
    }
    fn model(&self) -> Option<Model> { self.data.as_ref().map(|d| rules::model(d, now_ms(), &self.open_key)) }
    fn val(&self, f: impl Fn(&Inputs) -> &Entity<w::InputState>, cx: &App) -> String {
        self.inputs.as_ref().map(|i| f(i).read(cx).value().to_string()).unwrap_or_default()
    }
}

struct Handle(Entity<Hp>);
impl Global for Handle {}

fn handle(cx: &mut App) -> Entity<Hp> {
    if let Some(h) = cx.try_global::<Handle>() { return h.0.clone(); }
    let e = cx.new(Hp::new);
    cx.set_global(Handle(e.clone()));
    e
}

/// The screen with its text boxes made (a window is at hand).
fn ready(window: &mut Window, cx: &mut App) -> Entity<Hp> {
    let e = handle(cx);
    if e.read(cx).inputs.is_none() {
        let mk = |ph: &'static str, window: &mut Window, cx: &mut App| cx.new(|cx| w::InputState::new(window, cx).placeholder(ph));
        let inputs = Inputs {
            n_name: mk("e.g. Ali\u{2019}s PC", window, cx),
            k_name: mk("e.g. insta-scraper", window, cx),
            k_chat: mk("the chat\u{2019}s id", window, cx),
            k_exp: mk("YYYY-MM-DD HH:MM", window, cx),
            k_cap: mk("", window, cx),
            k_slots: mk("", window, cx),
            k_from: mk("homepool, loopback, 172.21.0.0/16", window, cx),
        };
        e.update(cx, |h, _| h.inputs = Some(inputs));
    }
    e
}

fn with<R>(cx: &mut App, f: impl FnOnce(&mut Hp, &mut Context<Hp>) -> R) -> R {
    let e = handle(cx);
    e.update(cx, |h, cx| { let r = f(h, cx); cx.notify(); r })
}

fn set_input(window: &mut Window, cx: &mut App, pick: impl Fn(&Inputs) -> &Entity<w::InputState>, v: &str) {
    let e = ready(window, cx);
    let i = pick(e.read(cx).inputs.as_ref().unwrap()).clone();
    let v = v.to_string();
    i.update(cx, |s, cx| s.set_value(v, window, cx));
}

fn now_ms() -> f64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0) }

/// A dropdown's labels live as long as the program (the widget takes
/// &'static str): each distinct one is kept once.
fn intern(s: &str) -> &'static str {
    thread_local! { static SEEN: RefCell<HashSet<&'static str>> = RefCell::new(HashSet::new()); }
    SEEN.with(|seen| {
        let mut seen = seen.borrow_mut();
        if let Some(x) = seen.get(s) { return *x; }
        let x: &'static str = Box::leak(s.to_string().into_boxed_str());
        seen.insert(x);
        x
    })
}
fn interned(v: &[String]) -> Vec<&'static str> { v.iter().map(|s| intern(s)).collect() }

/// A toast from anywhere (after the current update, on the window).
fn toast(cx: &mut App, text: &str, kind: w::Toast) {
    let text = text.to_string();
    cx.defer(move |cx| {
        if let Some(h) = cx.windows().first().copied() { let _ = cx.update_window(h, |_, window, cx| w::toast(window, cx, text, kind)); }
    });
}

// ─── The spine's hooks ───

pub fn view(tab: &str, window: &mut Window, cx: &mut App) -> Option<AnyView> {
    if tab != STEM { return None; }
    Some(ready(window, cx).into())
}

pub fn on_event(_e: &AreaEvent, _cx: &mut App) {}
pub fn on_project(_alias: &str, _cx: &mut App) {}
pub fn on_signed_in(_cx: &mut App) {}

pub fn on_signed_out(cx: &mut App) {
    if !cx.has_global::<Handle>() { return; }
    with(cx, |h, _| {
        h.shown = false; h.poll = None; h.data = None; h.error.clear(); h.tab = "networks".into(); h.open_key.clear();
        h.dlg = Dlg::None; h.dlg_title.clear(); h.dlg_label.clear();
    });
}

pub fn on_tab(tab: &str, cx: &mut App) {
    let was = handle(cx).read(cx).shown;
    if tab == STEM && !was {
        let poll = cx.spawn(async move |cx: &mut AsyncApp| loop {
            cx.background_executor().timer(Duration::from_millis(3000)).await;
            // Live while open; a dialog being filled in is left alone.
            let go = cx.update(|cx| { let h = handle(cx); let h = h.read(cx); h.shown && h.dlg == Dlg::None });
            if go { cx.update(load); }
        });
        with(cx, |h, _| { h.shown = true; h.poll = Some(poll); });
        load(cx);
    } else if tab != STEM && was {
        with(cx, |h, _| { h.shown = false; h.poll = None; });
    }
}

// ─── Data ───

fn load(cx: &mut App) {
    crundi_shell::call(cx, |api| api.get("/api/homepool"), |r, cx| {
        // The 3 s refresh that brings nothing new draws nothing.
        if let Ok(d) = &r { if handle(cx).read(cx).error.is_empty() && handle(cx).read(cx).data.as_ref() == Some(d) { return; } }
        with(cx, |h, _| match r {
        Err(e) if e == "Not supported by this server" => h.error = "This server does not support homepool yet. Update the server to use it.".into(),
        Err(e) if e.starts_with("Cannot reach") => h.error = "Could not reach the server.".into(),
        Err(e) => h.error = if e.is_empty() { "Could not load homepool.".into() } else { e },
        Ok(d) if d.get("ok") == Some(&Value::Bool(false)) => h.error = { let e = ss(&d, "error"); if e.is_empty() { "Could not load homepool.".into() } else { e } },
        Ok(d) => { h.error.clear(); h.data = Some(d); }
    })});
}

/// POST /api/homepool: the answer's overview redraws; `ok` is toasted.
fn act(cx: &mut App, payload: Value, ok: &str, then: impl FnOnce(Option<Value>, &mut App) + 'static) {
    let ok = ok.to_string();
    crundi_shell::call(cx, move |api| api.post("/api/homepool", &payload), move |r, cx| {
        let r = match r {
            Ok(d) if d.get("ok") == Some(&Value::Bool(false)) => Err(ss(&d, "error")),
            Err(e) if e.starts_with("Cannot reach") => Err("Could not reach the server".into()),
            x => x,
        };
        match r {
            Err(e) => { toast(cx, if e.is_empty() { "That did not work" } else { &e }, w::Toast::Error); then(None, cx); }
            Ok(d) => {
                if let Some(ov) = d.get("overview").filter(|x| x.is_object()) {
                    let mut ov = ov.clone();
                    ov["ok"] = json!(true);
                    with(cx, |h, _| h.data = Some(ov));
                }
                if !ok.is_empty() { toast(cx, &ok, w::Toast::Success); }
                then(Some(d), cx);
            }
        }
    });
}

fn data(cx: &mut App) -> Value { handle(cx).read(cx).data.clone().unwrap_or_default() }

fn copy(text: &str, cx: &mut App) {
    with(cx, |h, _| h.copied = text.to_string());
    cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
    toast(cx, "Copied", w::Toast::Success);
}

// ─── Actions (the Slint client's OpsHomepool.act and friends) ───

fn prompt(window: &mut Window, cx: &mut App, title: &str, message: &str, value: &str, then: impl Fn(String, &mut App) + 'static) {
    w::ask(window, cx, w::Ask { title: title.to_string().into(), message: message.to_string().into(), ok: "Save".into(), prompt: Some(value.into()), ..Default::default() }, move |v, _, cx| {
        if let Some(v) = v.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) { then(v, cx); }
    });
}

fn confirm(window: &mut Window, cx: &mut App, title: String, message: &str, ok: &str, danger: bool, then: impl Fn(&mut App) + 'static) {
    w::ask_confirm(window, cx, w::Ask { title: title.into(), message: message.to_string().into(), ok: ok.to_string().into(), danger, ..Default::default() }, move |ok, _, cx| { if ok { then(cx); } });
}

fn set_tab(tab: &str, cx: &mut App) { let t = tab.to_string(); with(cx, |h, _| h.tab = t); }

fn action(a: &str, id: &str, window: &mut Window, cx: &mut App) {
    let d = data(cx);
    let find = |k: &str| arr(&d, k).into_iter().find(|x| ss(x, "id") == id).cloned().unwrap_or_default();
    let (node, key, req) = (find("nodes"), find("keys"), find("requests"));
    let id = id.to_string();
    match a {
        "net-add" => prompt(window, cx, "Add a network", "Name (where it is: Home, Ali\u{2019}s place, Office)", "", |v, cx| act(cx, json!({ "action": "network-add", "name": v }), "Network added", |_, _| {})),
        "net-rename" => prompt(window, cx, "Rename network", "Name", &rules::net_name(&d, &id), move |v, cx| act(cx, json!({ "action": "network-rename", "id": id, "name": v }), "", |_, _| {})),
        "net-remove" => confirm(window, cx, format!("Remove \"{}\"?", rules::net_name(&d, &id)), "Its nodes stay, but carry nothing until you put them in another network. Keys lose this network.", "Remove", true,
            move |cx| act(cx, json!({ "action": "network-remove", "id": id }), "Removed", |_, _| {})),
        "node-add" => open_node_dialog(window, cx),
        "node-rename" => prompt(window, cx, "Rename node", "Name", &ss(&node, "name"), move |v, cx| act(cx, json!({ "action": "node-update", "id": id, "name": v }), "", |_, _| {})),
        "node-reissue" => {
            let name = ss(&node, "name");
            confirm(window, cx, format!("New key for \"{name}\"?"), "The old key stops working now and the node disconnects. Set it up again with the new code.", "New key", false, move |cx| {
                let (id, name) = (id.clone(), name.clone());
                act(cx, json!({ "action": "node-reissue", "id": id }), "", move |r, cx| { if let Some(r) = r { show_node_setup(&r["node"], &format!("New key for \"{name}\""), cx); } });
            })
        }
        "node-remove" => confirm(window, cx, format!("Remove \"{}\"?", ss(&node, "name")), "It disconnects now and its key stops working for good.", "Remove", true,
            move |cx| act(cx, json!({ "action": "node-remove", "id": id }), "Removed", |_, _| {})),
        "key-add" => open_key_dialog(None, None, window, cx),
        "key-edit" => open_key_dialog(Some(key), None, window, cx),
        "key-use" => with(cx, |h, _| h.open_key = if h.open_key == id { String::new() } else { id.clone() }),
        "key-reissue" => {
            let name = ss(&key, "name");
            confirm(window, cx, format!("New secret for \"{name}\"?"), "The old one stops working now.", "New secret", false, move |cx| {
                let (id, name) = (id.clone(), name.clone());
                act(cx, json!({ "action": "key-reissue", "id": id }), "", move |r, cx| { if let Some(r) = r { show_secret(&r, &name, cx); } });
            })
        }
        "key-remove" => confirm(window, cx, format!("Revoke \"{}\"?", ss(&key, "name")), "Its open connections end now and it can never be used again.", "Revoke", true,
            move |cx| act(cx, json!({ "action": "key-remove", "id": id }), "Revoked", |_, _| {})),
        "req-approve" => act(cx, json!({ "action": "request-decide", "id": id, "approve": true }), "Approved", |_, _| {}),
        "req-refuse" => act(cx, json!({ "action": "request-decide", "id": id, "approve": false }), "Refused", |_, _| {}),
        "req-change" => {
            if req.is_null() { return; }
            let k = json!({ "kind": ss(&req, "kind"), "name": ss(&req, "name"), "project": ss(&req, "project"), "chatId": ss(&req, "chatId"), "networks": req.get("networks").cloned().unwrap_or(json!([])), "slots": 8 });
            open_key_dialog(Some(k), Some(id), window, cx);
        }
        _ => {}
    }
}

fn node_move(id: &str, i: usize, cx: &mut App) {
    let net = if i == 0 { String::new() } else { home_nets(&data(cx)).get(i - 1).map(|n| ss(n, "id")).unwrap_or_default() };
    act(cx, json!({ "action": "node-update", "id": id, "network": net }), "Moved", |_, _| {});
}

fn toggle(kind: &str, id: &str, on: bool, cx: &mut App) {
    match kind {
        "pool" => act(cx, json!({ "action": "settings", "settings": { "poolBridge": { "enabled": on } } }), if on { "Standalone homepool on" } else { "Standalone homepool off" }, |_, _| {}),
        "node" => act(cx, json!({ "action": "node-update", "id": id, "enabled": on }), "", |_, _| {}),
        _ => act(cx, json!({ "action": "key-update", "id": id, "key": { "enabled": on } }), if on { "Switched on" } else { "Switched off: its connections are cut" }, |_, _| {}),
    }
}

// ─── Dialogs ───

fn set_dlg(h: &mut Hp, d: Dlg, title: &str, ok: &str) {
    h.dlg = d;
    h.dlg_title = title.into();
    h.dlg_label = ok.into();
    h.scroll = ScrollHandle::new();
}

fn close_dlg(cx: &mut App) { with(cx, |h, _| set_dlg(h, Dlg::None, "", "")); }

fn open_node_dialog(window: &mut Window, cx: &mut App) {
    let nets = home_nets(&data(cx));
    let sel = nets.iter().position(|n| ss(n, "id") == "home").map(|i| i + 1).unwrap_or(0);
    set_input(window, cx, |i| &i.n_name, "");
    let fh = with(cx, |h, _| {
        h.n_nets = std::iter::once("No network yet".to_string()).chain(nets.iter().map(|n| ss(n, "name"))).collect();
        h.n_net_ids = std::iter::once(String::new()).chain(nets.iter().map(|n| ss(n, "id"))).collect();
        h.n_net = sel;
        set_dlg(h, Dlg::Node, "Add a node", "Make node");
        h.inputs.as_ref().unwrap().n_name.clone()
    });
    fh.update(cx, |s, cx| s.focus(window, cx));
}

fn open_key_dialog(k: Option<Value>, request: Option<String>, window: &mut Window, cx: &mut App) {
    let edit = k.as_ref().filter(|k| !ss(k, "id").is_empty() && request.is_none()).map(|k| ss(k, "id"));
    let k = k.unwrap_or_else(|| json!({ "kind": "software", "networks": ["server"], "slots": 8 }));
    let kind = ["software", "project", "chat"].iter().position(|x| *x == ss(&k, "kind")).unwrap_or(0);
    let projects = session::get(cx).projects.clone();
    let labels: Vec<String> = std::iter::once("\u{2014}".to_string()).chain(projects.iter().map(|p| { let n = ss(p, "name"); if n.is_empty() { ss(p, "alias") } else { n } })).collect();
    let pids: Vec<String> = std::iter::once(String::new()).chain(projects.iter().map(|p| ss(p, "alias"))).collect();
    let psel = pids.iter().position(|a| !a.is_empty() && *a == ss(&k, "project")).unwrap_or(0);
    let d = data(cx);
    let nets: Vec<String> = arr(&d, "networks").into_iter().map(|n| ss(n, "id")).collect();
    let want: Vec<String> = arr(&k, "networks").into_iter().map(|x| x.as_str().unwrap_or("").to_string()).collect();
    let slots = match k.get("slots") { Some(Value::Null) | None => "8".to_string(), Some(x) => x.to_string() };
    let from = { let a: Vec<String> = arr(&k, "allowFrom").into_iter().map(|x| x.as_str().unwrap_or("").to_string()).collect(); if a.is_empty() { "homepool".to_string() } else { a.join(", ") } };
    set_input(window, cx, |i| &i.k_name, &ss(&k, "name"));
    set_input(window, cx, |i| &i.k_chat, &ss(&k, "chatId"));
    set_input(window, cx, |i| &i.k_exp, &k.get("expiresAt").and_then(|x| x.as_f64()).map(rules::show_local).unwrap_or_default());
    set_input(window, cx, |i| &i.k_cap, &format!("{}", rules::n(&k, "dailyCapMb") as i64));
    set_input(window, cx, |i| &i.k_slots, &slots);
    set_input(window, cx, |i| &i.k_from, &from);
    let (title, ok) = if edit.is_some() { (format!("Change access \"{}\"", ss(&k, "name")), "Save") } else if request.is_some() { ("Approve with changes".to_string(), "Approve") } else { ("Give access".to_string(), "Give access") };
    with(cx, |h, _| {
        h.k_kind = kind;
        h.k_kind_locked = edit.is_some();
        h.k_projects = labels;
        h.k_project_ids = pids;
        h.k_project = psel;
        h.k_project_locked = edit.is_some() || request.is_some();
        h.k_nets_on = nets.iter().map(|n| want.contains(n)).collect();
        h.k_net_ids = nets;
        h.k_fb = t(&k, "fallbackToServer");
        h.k_block = t(&k, "portBlock");
        set_dlg(h, Dlg::Key { edit, request }, &title, ok);
    });
}

/// readKey(): the access dialog as the server takes it.
fn read_key(cx: &mut App) -> Value {
    let e = handle(cx);
    let h = e.read(cx);
    let kind = ["software", "project", "chat"][h.k_kind.min(2)];
    let nets: Vec<String> = h.k_net_ids.iter().zip(&h.k_nets_on).filter(|(_, on)| **on).map(|(id, _)| id.clone()).collect();
    let project = h.k_project_ids.get(h.k_project).cloned().unwrap_or_default();
    let name = h.val(|i| &i.k_name, cx).trim().to_string();
    let mut out = json!({
        "kind": kind,
        "name": name,
        "project": project,
        "chatId": h.val(|i| &i.k_chat, cx).trim(),
        "networks": nets,
        "fallbackToServer": h.k_fb,
        "expiresAt": rules::parse_local(&h.val(|i| &i.k_exp, cx)),
        "dailyCapMb": h.val(|i| &i.k_cap, cx).trim().parse::<f64>().unwrap_or(0.0),
        "slots": h.val(|i| &i.k_slots, cx).trim().parse::<f64>().unwrap_or(0.0),
        "portBlock": kind == "software" && h.k_block,
        "allowFrom": h.val(|i| &i.k_from, cx).split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>(),
    });
    if name.is_empty() { out.as_object_mut().unwrap().remove("name"); }
    out
}

fn dlg_ok(cx: &mut App) {
    let e = handle(cx);
    let dlg = e.read(cx).dlg.clone();
    match dlg {
        Dlg::Node => {
            let (name, net) = { let h = e.read(cx); (h.val(|i| &i.n_name, cx).trim().to_string(), h.n_net_ids.get(h.n_net).cloned().unwrap_or_default()) };
            if name.is_empty() { toast(cx, "Give the node a name", w::Toast::Error); return; }
            act(cx, json!({ "action": "node-add", "name": name, "network": net }), "", move |r, cx| {
                if let Some(r) = r { close_dlg(cx); show_node_setup(&r["node"], &format!("Node \"{name}\" is ready"), cx); }
            });
        }
        Dlg::Key { edit, request } => {
            let key = read_key(cx);
            if arr(&key, "networks").is_empty() { toast(cx, "Pick at least one network", w::Toast::Error); return; }
            if let Some(req) = request {
                act(cx, json!({ "action": "request-decide", "id": req, "approve": true, "changes": key }), "Approved", |r, cx| { if r.is_some() { close_dlg(cx); } });
            } else if let Some(id) = edit {
                act(cx, json!({ "action": "key-update", "id": id, "key": key }), "Saved", |r, cx| { if r.is_some() { close_dlg(cx); } });
            } else {
                let name = { let n = ss(&key, "name"); if n.is_empty() { "Access".to_string() } else { n } };
                act(cx, json!({ "action": "key-add", "key": key }), "", move |r, cx| {
                    if let Some(r) = r {
                        close_dlg(cx);
                        if !ss(&r, "secret").is_empty() { show_secret(&r, &name, cx); }
                    }
                });
            }
        }
        _ => close_dlg(cx),
    }
}

fn show_node_setup(node: &Value, title: &str, cx: &mut App) {
    let code = { let c = ss(node, "code"); if c.is_empty() { ss(node, "key") } else { c } };
    let mut items = vec![use_line("CODE (SCAN OR PASTE IN THE HOMEPOOL NODE APP OR THE CRUNDI APP)", "code", &code)];
    let docker = ss(node, "docker");
    if !docker.is_empty() { items.push(use_line("OR ON ANY MACHINE WITH DOCKER", "code", &docker)); }
    let qr = rules::qr_runs(&ss(node, "qr"));
    let warn2 = if ss(node, "server").is_empty() { "This server has no public address set, so the code has none. Set one (TLS domain or tunnel) for nodes outside this machine." } else { "" };
    let title = title.to_string();
    with(cx, |h, _| {
        h.show_warn = "The key is shown this once. Whoever has it can lend their connection to this server as this node (and nothing else). Give it only to the person running the node.".into();
        h.show_items = items;
        h.qr = qr;
        h.show_warn2 = warn2.into();
        set_dlg(h, Dlg::Show, &title, "");
    });
}

fn show_secret(r: &Value, name: &str, cx: &mut App) {
    let u = r.get("use").cloned().unwrap_or_default();
    let mut items = vec![use_line("LOGIN", "code", &format!("user: {}\npassword: {}", ss(&r["key"], "id"), ss(r, "secret")))];
    if !ss(&u, "socks").is_empty() { items.push(use_line("SOCKS5", "code", &ss(&u, "socks"))); }
    if !ss(&u, "http").is_empty() { items.push(use_line("HTTP PROXY", "code", &ss(&u, "http"))); }
    if let Some(b) = u.get("block").filter(|b| b.is_object()) {
        items.push(use_line("PORT BLOCK (NO LOGIN)", "code", &arr(b, "compose").iter().map(|x| x.as_str().unwrap_or("")).collect::<Vec<_>>().join("\n")));
    }
    let title = format!("Access \"{name}\"");
    with(cx, |h, _| {
        h.show_warn = "The secret is shown this once. Keep it with the software (its .env or a Crundi secret), never in a file you commit.".into();
        h.show_items = items;
        h.qr = None;
        h.show_warn2 = String::new();
        set_dlg(h, Dlg::Show, &title, "");
    });
}

// ─── Drawing ───

fn eid(prefix: &str, id: &str) -> ElementId { ElementId::Name(format!("hp-{prefix}-{id}").into()) }

fn meta(text: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_12).text_color(T::text_muted()).child(text.into()) }

fn chip(text: impl Into<SharedString>, tone: &str) -> Div { w::tag(text, rules::tag_tone(tone)) }

/// A card (.hp-card): its head (title and chips, the actions on the right) and its body.
fn hp_card(head: Vec<AnyElement>, body: Div) -> Div {
    div().flex().flex_col().flex_none().rounded(T::R_FIELD).border_1().border_color(T::border()).bg(T::bg_secondary()).overflow_hidden()
        .child(div().flex().flex_wrap().items_center().gap(T::SP_8).min_h(T::CONTROL + T::SP_4).px(T::SP_12).py(T::SP_8).border_b_1().border_color(T::border_subtle()).children(head))
        .child(body.px(T::SP_12).pt(T::SP_8).pb(T::SP_10))
}
fn hp_title(text: impl Into<SharedString>) -> AnyElement { div().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(text.into()).into_any_element() }
fn spacer() -> AnyElement { div().flex_1().min_w_0().into_any_element() }

/// "code": a mono box and its Copy (.hp-code); "quiet": muted words; "warn": the yellow note.
fn use_el(key: String, u: &Use) -> Div {
    let mut d = div().flex().flex_col().gap(T::SP_4);
    if !u.label.is_empty() { d = d.child(w::field_label(u.label.clone())); }
    match u.kind {
        "quiet" => d.child(meta(u.text.clone())),
        "warn" => d.child(div().p(T::SP_8).rounded(T::R_FIELD).border_1().border_color(T::yellow()).bg(T::yellow_dim()).text_size(T::TEXT_12).text_color(T::text()).child(u.text.clone())),
        _ => {
            let text = u.text.clone();
            d.child(div().p(T::SP_8).rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg_tertiary()).font_family(T::MONO).text_size(T::TEXT_12).text_color(T::text()).child(u.text.clone()))
                .child(div().flex().child(w::button(eid("copy", &key), "Copy").icon(IconName::Copy).on_click(move |_, _, cx| copy(&text, cx))))
        }
    }
}

fn node_row(n: &rules::Node, move_opts: &[&'static str], first: bool) -> Div {
    let dot = match n.dot { "up" => T::green(), "paused" => T::yellow(), _ => T::text_muted() };
    let (id1, id2, id3, id4, id5) = (n.id.clone(), n.id.clone(), n.id.clone(), n.id.clone(), n.id.clone());
    div().flex().gap(T::SP_10).py(T::SP_8).when(!first, |d| d.border_t_1().border_color(T::border_subtle()))
        .child(div().pt(T::SP_4).child(w::dot(dot)))
        .child(div().flex().flex_col().flex_1().min_w_0().gap(T::SP_4)
            .child(div().flex().items_center().gap(T::SP_6)
                .child(div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(n.name.clone()))
                .child(chip(n.state.clone(), n.tone)))
            .child(meta(n.meta.clone()))
            .child(div().flex().flex_wrap().items_center().gap(T::SP_6)
                .child(w::dropdown(eid("move", &n.id), move_opts, n.net, false, move |i, _, cx| node_move(&id1, i, cx)))
                .child(w::switch(eid("on", &n.id), n.on, move |on, _, cx| toggle("node", &id2, *on, cx)).tone("green"))
                .child(w::button(eid("rename", &n.id), "Rename").on_click(move |_, window, cx| action("node-rename", &id3, window, cx)))
                .child(w::button(eid("reissue", &n.id), "New key").on_click(move |_, window, cx| action("node-reissue", &id4, window, cx)))
                .child(w::danger(eid("remove", &n.id), "Remove").on_click(move |_, window, cx| action("node-remove", &id5, window, cx)))))
}

fn networks(m: &Model) -> Vec<AnyElement> {
    let opts = interned(&m.move_opts);
    let mut out = vec![meta("Traffic leaves only through a network a key was given. Put each node in the network it really is on: two phones on one home Wi-Fi share one address.").into_any_element()];
    for n in &m.nets {
        let mut head = vec![hp_title(n.name.clone()), chip(n.id.clone(), "").into_any_element()];
        if !n.chip.is_empty() { head.push(chip(n.chip.clone(), n.tone).into_any_element()); }
        head.push(spacer());
        if n.kind == "pool" { head.push(w::switch("hp-pool-on", n.on, |on, _, cx| toggle("pool", "pool", *on, cx)).tone("green").into_any_element()); }
        if n.kind == "own" {
            let (a, b) = (n.id.clone(), n.id.clone());
            head.push(w::button(eid("net-rename", &n.id), "Rename").on_click(move |_, window, cx| action("net-rename", &a, window, cx)).into_any_element());
            head.push(w::danger(eid("net-remove", &n.id), "Remove").on_click(move |_, window, cx| action("net-remove", &b, window, cx)).into_any_element());
        }
        let mut body = div().flex().flex_col();
        if !n.body.is_empty() { body = body.child(meta(n.body.clone())); }
        if !n.err.is_empty() { body = body.child(div().text_size(T::TEXT_12).text_color(T::red()).child(n.err.clone())); }
        for (i, x) in n.nodes.iter().enumerate() { body = body.child(node_row(x, &opts, i == 0)); }
        out.push(hp_card(head, body).into_any_element());
    }
    if !m.loose.is_empty() {
        let head = vec![hp_title("Not in a network"), meta("carry nothing until you put them in one").into_any_element(), spacer()];
        let mut body = div().flex().flex_col();
        for (i, x) in m.loose.iter().enumerate() { body = body.child(node_row(x, &opts, i == 0)); }
        out.push(hp_card(head, body).into_any_element());
    }
    out
}

fn access(m: &Model) -> Vec<AnyElement> {
    if m.keys.is_empty() { return vec![meta("No access given yet, so nothing can use homepool. Add access for a chat, a project or a piece of software, or approve a request from a chat.").into_any_element()]; }
    m.keys.iter().map(|k| {
        let mut head = vec![hp_title(k.name.clone())];
        head.extend(k.nets.iter().map(|n| chip(n.clone(), "").into_any_element()));
        head.push(chip(k.state.clone(), k.tone).into_any_element());
        head.push(spacer());
        let id = k.id.clone();
        head.push(w::switch(eid("key-on", &k.id), k.on, move |on, _, cx| toggle("key", &id, *on, cx)).tone("green").into_any_element());
        let (a, b, c, e) = (k.id.clone(), k.id.clone(), k.id.clone(), k.id.clone());
        let mut btns = div().flex().flex_wrap().gap(T::SP_6)
            .child(w::button(eid("key-use", &k.id), if k.open { "Hide" } else { "How to use" }).on_click(move |_, window, cx| action("key-use", &a, window, cx)))
            .child(w::button(eid("key-edit", &k.id), "Change").icon(IconName::Pencil).on_click(move |_, window, cx| action("key-edit", &b, window, cx)));
        if k.software { btns = btns.child(w::button(eid("key-reissue", &k.id), "New secret").on_click(move |_, window, cx| action("key-reissue", &c, window, cx))); }
        btns = btns.child(w::danger(eid("key-remove", &k.id), "Revoke").icon(IconName::Trash).on_click(move |_, window, cx| action("key-remove", &e, window, cx)));
        let mut body = div().flex().flex_col().gap(T::SP_6).child(meta(k.meta.clone())).child(btns);
        if k.open {
            body = body.child(div().flex().flex_col().pt(T::SP_4).gap(T::SP_6).children(k.uses.iter().enumerate().map(|(i, u)| use_el(format!("{}-{i}", k.id), u))));
        }
        hp_card(head, body).into_any_element()
    }).collect()
}

fn requests(m: &Model) -> Vec<AnyElement> {
    if m.reqs.is_empty() { return vec![meta("No requests. A chat asks with the MCP tool homepool_request, and nothing is made until you approve it here.").into_any_element()]; }
    m.reqs.iter().map(|r| {
        let mut head = vec![hp_title(r.title.clone())];
        head.extend(r.nets.iter().map(|n| chip(n.clone(), "").into_any_element()));
        head.push(chip(r.status.clone(), r.tone).into_any_element());
        head.push(spacer());
        let mut body = div().flex().flex_col().gap(T::SP_6).child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(r.meta.clone()));
        if !r.reason.is_empty() { body = body.child(div().text_size(T::TEXT_13).text_color(T::text()).child(r.reason.clone())); }
        if r.pending {
            let (a, b, c) = (r.id.clone(), r.id.clone(), r.id.clone());
            body = body.child(div().flex().flex_wrap().gap(T::SP_6)
                .child(w::primary(eid("req-approve", &r.id), "Approve").icon(IconName::Check).on_click(move |_, window, cx| action("req-approve", &a, window, cx)))
                .child(w::button(eid("req-change", &r.id), "Change and approve").on_click(move |_, window, cx| action("req-change", &b, window, cx)))
                .child(w::danger(eid("req-refuse", &r.id), "Refuse").on_click(move |_, window, cx| action("req-refuse", &c, window, cx))));
        }
        hp_card(head, body).when(!r.pending, |d| d.opacity(T::HOMEPOOL_O_SOFT)).into_any_element()
    }).collect()
}

fn activity(m: &Model) -> Vec<AnyElement> {
    let mut d = div().flex().flex_col().flex_none().gap(T::SP_4).p(T::SP_12).rounded(T::R_FIELD).border_1().border_color(T::border()).bg(T::bg_secondary());
    if m.log.is_empty() { d = d.child(meta("Nothing yet.")); }
    for e in &m.log {
        d = d.child(div().flex().gap(T::SP_8).font_family(T::MONO).text_size(T::TEXT_12)
            .child(div().flex_none().text_color(T::text_muted()).child(e.at.clone()))
            .child(div().flex_none().text_color(T::text()).child(e.what.clone()))
            .when(!e.subj.is_empty(), |d| d.child(div().flex_1().min_w_0().truncate().font_weight(T::W_BOLD).text_color(T::text()).child(e.subj.clone()))));
    }
    vec![d.into_any_element()]
}

/// A field: its label over the control (and a hint under it).
fn field(label: &str, control: impl IntoElement, hint: &str) -> Div {
    div().flex().flex_col().gap(T::SP_4).child(w::field_label(label.to_string())).child(control).when(!hint.is_empty(), |d| d.child(meta(hint.to_string())))
}

impl Hp {
    fn dialog_body(&self) -> Vec<AnyElement> {
        let Some(i) = &self.inputs else { return vec![] };
        let mut out: Vec<AnyElement> = vec![];
        match &self.dlg {
            Dlg::Node => {
                out.push(field("NAME", w::input(&i.n_name), "").into_any_element());
                let nets = interned(&self.n_nets);
                out.push(field("NETWORK", div().flex().child(w::dropdown("hp-n-net", &nets, self.n_net, false, |i, _, cx| with(cx, |h, _| h.n_net = i))),
                    "A friend\u{2019}s line is a network of its own: make one first if it does not exist (Network button).").into_any_element());
            }
            Dlg::Key { .. } => {
                out.push(field("FOR", div().flex().child(w::dropdown("hp-k-kind", &["Software", "Project", "Chat"], self.k_kind, self.k_kind_locked, |i, _, cx| with(cx, |h, _| h.k_kind = i))),
                    "Software: a scraper, script or container, with a login of its own. Project: every chat of a project. Chat: one chat.").into_any_element());
                if self.k_kind == 0 { out.push(field("NAME", w::input(&i.k_name), "").into_any_element()); }
                if self.k_kind != 0 {
                    let projects = interned(&self.k_projects);
                    out.push(field("PROJECT", div().flex().child(w::dropdown("hp-k-project", &projects, self.k_project, self.k_project_locked, |i, _, cx| with(cx, |h, _| h.k_project = i))), "").into_any_element());
                }
                if self.k_kind == 2 { out.push(field("CHAT ID", w::input(&i.k_chat).disabled(self.k_project_locked), "Easier: let the chat ask with homepool_request, then approve it.").into_any_element()); }
                let d = self.data.clone().unwrap_or_default();
                let checks = div().flex().flex_wrap().gap(T::SP_12).children(self.k_net_ids.iter().zip(&self.k_nets_on).enumerate().map(|(n, (id, on))| {
                    w::check(eid("k-net", id), rules::net_name(&d, id), *on, move |on, _, cx| { let on = *on; with(cx, |h, _| if let Some(x) = h.k_nets_on.get_mut(n) { *x = on; }) })
                }));
                out.push(field("NETWORKS IT MAY USE", checks, "With more than one, its slots spread over all of them and each slot stays on one exit, as the standalone homepool does. A login can pin one with user@network.").into_any_element());
                out.push(div().flex().flex_col().gap(T::SP_4)
                    .child(w::check("hp-k-fb", "Fall back to the server when none of them is up", self.k_fb, |on, _, cx| { let on = *on; with(cx, |h, _| h.k_fb = on) }))
                    .child(meta("Off: with no exit up, connections are refused rather than quietly leaving from the server.")).into_any_element());
                out.push(field("EXPIRES", w::input(&i.k_exp), "Empty: until you revoke it.").into_any_element());
                out.push(div().flex().gap(T::SP_12)
                    .child(field("DATA CAP PER DAY (MB, 0 = NONE)", w::input(&i.k_cap), "").flex_1().min_w_0())
                    .child(field("SLOTS", w::input(&i.k_slots), "").flex_1().min_w_0()).into_any_element());
                out.push(meta("Slots: identities that should each keep one exit (one per account).").into_any_element());
                if self.k_kind == 0 {
                    out.push(div().flex().flex_col().gap(T::SP_6)
                        .child(w::check("hp-k-block", "Port block, for software that cannot send a proxy login (Chromium with SOCKS)", self.k_block, |on, _, cx| { let on = *on; with(cx, |h, _| h.k_block = on) }))
                        .child(w::input(&i.k_from))
                        .child(meta("Who may use the ports: Docker network names, loopback, or address ranges. Nothing else can.")).into_any_element());
                }
            }
            Dlg::Show => {
                if !self.show_warn.is_empty() { out.push(use_el("warn".into(), &use_line("", "warn", &self.show_warn)).into_any_element()); }
                if let Some((runs, n)) = self.qr.clone() {
                    out.push(div().flex().child(div().size(T::HOMEPOOL_QR).p(T::SP_8).rounded(T::R_FIELD).bg(T::white())
                        .child(canvas(|_, _, _| (), move |b, _, window, _| {
                            let m = b.size.width / n;
                            for (x, y, w) in &runs {
                                window.paint_quad(fill(Bounds::new(point(b.origin.x + m * *x, b.origin.y + m * *y), size(m * *w, m)), T::bg()));
                            }
                        }).size_full())).into_any_element());
                }
                out.push(div().flex().flex_col().gap(T::SP_10).children(self.show_items.iter().enumerate().map(|(n, u)| use_el(format!("show-{n}"), u))).into_any_element());
                if !self.show_warn2.is_empty() { out.push(use_el("warn2".into(), &use_line("", "warn", &self.show_warn2)).into_any_element()); }
            }
            Dlg::None => {}
        }
        out
    }

    fn dialog(&self, window: &Window) -> AnyElement {
        let mut acts = div().flex().justify_end().gap(T::SP_8).pt(T::SP_12).px(T::SP_8).pb(T::SP_4);
        if self.dlg_label.is_empty() {
            acts = acts.child(w::primary("hp-dlg-done", "Done").on_click(|_, _, cx| close_dlg(cx)));
        } else {
            acts = acts.child(w::button("hp-dlg-cancel", "Cancel").on_click(|_, _, cx| close_dlg(cx)))
                .child(w::primary("hp-dlg-ok", self.dlg_label.clone()).on_click(|_, _, cx| dlg_ok(cx)));
        }
        let children = vec![
            // .hp-dlg h3: the title alone (no ×: Cancel / Done, Esc and the scrim close it).
            div().px(T::SP_8).pt(T::SP_8).pb(T::SP_10).text_size(T::TEXT_16).font_weight(T::W_BOLD).text_color(T::text()).child(self.dlg_title.clone()).into_any_element(),
            div().flex().flex_col().gap(T::SP_12).px(T::SP_8).children(self.dialog_body()).into_any_element(),
            acts.into_any_element(),
        ];
        let m = w::Modal {
            id: "hp-dlg", width: T::HOMEPOOL_DLG_W, focus: self.focus.clone(), scroll: self.scroll.clone(),
            on_close: Rc::new(|_, cx| close_dlg(cx)),
            on_key: Rc::new(|e, _, cx| if e.keystroke.key == "escape" { close_dlg(cx) }),
        };
        w::overlay_modal(m, window, children).into_any_element()
    }
}

impl Render for Hp {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("homepool");
        let Some(m) = self.model() else {
            let text = if self.error.is_empty() { "Loading\u{2026}".to_string() } else { self.error.clone() };
            return div().size_full().bg(T::bg()).child(w::page("hp-page").child(meta(text))).into_any_element();
        };
        let on = m.tabs.iter().position(|t| t.key == self.tab).unwrap_or(0);
        let keys: Vec<&'static str> = m.tabs.iter().map(|t| t.key).collect();
        let tabs = m.tabs.iter().map(|t| w::SegTab { icon: None, label: t.label.into(), count: t.count.clone().into(), alert: t.alert }).collect();
        let mut bar = div().flex().flex_none().items_center().gap(T::SP_8).px(T::SP_16).py(T::SP_10).border_b_1().border_color(T::border())
            .child(w::seg_tabs("hp-tabs", tabs, on, move |i, _, cx| set_tab(keys[i], cx)))
            .child(div().flex_1().min_w_0());
        match self.tab.as_str() {
            "networks" => {
                bar = bar.child(w::button("hp-net-add", "Network").icon(IconName::Plus).on_click(|_, window, cx| action("net-add", "", window, cx)))
                    .child(w::primary("hp-node-add", "Node").icon(IconName::Plus).on_click(|_, window, cx| action("node-add", "", window, cx)));
            }
            "access" => { bar = bar.child(w::primary("hp-key-add", "Access").icon(IconName::Plus).on_click(|_, window, cx| action("key-add", "", window, cx))); }
            _ => {}
        }
        let sum = div().flex().flex_none().gap(T::SP_20).children(m.sum.iter().map(|(v, l)| div().flex().flex_col()
            .child(div().font_family(T::MONO).text_size(T::TEXT_16).text_color(T::text()).child(v.clone()))
            .child(div().text_size(T::TEXT_10).text_color(T::text_muted()).child(l.clone()))));
        let body = match self.tab.as_str() { "access" => access(&m), "requests" => requests(&m), "activity" => activity(&m), _ => networks(&m) };
        div().size_full().flex().flex_col().bg(T::bg())
            .child(bar)
            .child(w::page("hp-page").track_scroll(&self.page_scroll).child(sum).children(body))
            .when(self.dlg != Dlg::None, |d| d.child(self.dialog(window)))
            .into_any_element()
    }
}

// ─── The test driver ───

fn arg(a: &[String], i: usize) -> &str { a.get(i).map(|s| s.as_str()).unwrap_or("") }

/// The driver's "ops:<name>" actions this screen knows (None: not mine).
pub fn test_ui(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    if !name.starts_with("homepool-") { return None; }
    let e = ready(window, cx);
    let ok = Some(json!({ "ok": true }));
    match name {
        "homepool-load" => { load(cx); ok }
        "homepool-tab" => { set_tab(arg(args, 0), cx); ok }
        "homepool-act" => { action(arg(args, 0), arg(args, 1), window, cx); ok }
        "homepool-move" => { node_move(arg(args, 0), arg(args, 1).parse().unwrap_or(0), cx); ok }
        "homepool-toggle" => { toggle(arg(args, 0), arg(args, 1), arg(args, 2) == "true", cx); ok }
        "homepool-copy" => { copy(arg(args, 0), cx); Some(json!({ "ok": true, "text": e.read(cx).copied.clone() })) }
        // The node dialog: name, network index.
        "homepool-node-fill" => {
            set_input(window, cx, |i| &i.n_name, arg(args, 0));
            let n = arg(args, 1).parse().unwrap_or(0);
            with(cx, |h, _| h.n_net = n);
            ok
        }
        // The access dialog: kind index, name, network ids (comma), fallback, cap, slots, expires, project index, chat id.
        "homepool-key-fill" => {
            let want: Vec<String> = arg(args, 2).split(',').filter(|x| !x.is_empty()).map(String::from).collect();
            let (kind, fb, project) = (arg(args, 0).parse().unwrap_or(0), arg(args, 3) == "true", arg(args, 7).parse().unwrap_or(0));
            let locked = with(cx, |h, _| {
                if !h.k_kind_locked { h.k_kind = kind; }
                h.k_nets_on = h.k_net_ids.iter().map(|id| want.contains(id)).collect();
                h.k_fb = fb;
                if !h.k_project_locked { h.k_project = project; }
                h.k_project_locked
            });
            set_input(window, cx, |i| &i.k_name, arg(args, 1));
            set_input(window, cx, |i| &i.k_cap, arg(args, 4));
            set_input(window, cx, |i| &i.k_slots, arg(args, 5));
            set_input(window, cx, |i| &i.k_exp, arg(args, 6));
            if !locked { set_input(window, cx, |i| &i.k_chat, arg(args, 8)); }
            ok
        }
        "homepool-dlg-ok" => { dlg_ok(cx); ok }
        "homepool-dlg-close" => { close_dlg(cx); ok }
        _ => None,
    }
}

/// The driver's view of this screen (the Slint client's props).
pub fn props(_window: &mut Window, cx: &mut App) -> Value {
    let e = handle(cx);
    let h = e.read(cx);
    let m = h.model();
    let state = if m.is_some() { "data" } else if h.error.is_empty() { "loading" } else { "error" };
    let m = m.unwrap_or_default();
    let node = |n: &rules::Node| json!({ "id": n.id, "name": n.name, "state": n.state, "meta": n.meta, "net": n.net, "on": n.on });
    let d = h.data.clone().unwrap_or_default();
    let dlg = match h.dlg { Dlg::None => "", Dlg::Node => "node", Dlg::Key { .. } => "key", Dlg::Show => "show" };
    json!({
        "shown": h.shown, "polling": h.poll.is_some(),
        "state": state, "error": h.error, "tab": h.tab,
        "tabs": m.tabs.iter().map(|t| json!([t.label, t.count, t.alert])).collect::<Vec<_>>(),
        "sum": m.sum.iter().map(|(v, l)| json!([v, l])).collect::<Vec<_>>(),
        "nets": m.nets.iter().map(|n| json!({ "id": n.id, "name": n.name, "kind": n.kind, "chip": n.chip, "on": n.on, "body": n.body, "nodes": n.nodes.iter().map(node).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "loose": m.loose.iter().map(node).collect::<Vec<_>>(),
        "moveOpts": m.move_opts,
        "keys": m.keys.iter().map(|k| json!({ "id": k.id, "name": k.name, "state": k.state, "on": k.on, "meta": k.meta, "open": k.open, "nets": k.nets, "uses": k.uses.iter().map(|u| u.text.clone()).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "reqs": m.reqs.iter().map(|r| json!({ "id": r.id, "title": r.title, "status": r.status, "pending": r.pending, "meta": r.meta })).collect::<Vec<_>>(),
        "log": m.log.iter().map(|e| format!("{} {}", e.what, e.subj)).collect::<Vec<_>>(),
        "dlg": dlg, "dlgTitle": h.dlg_title, "dlgOk": h.dlg_label, "modal": dlg != "",
        "kNets": h.k_net_ids.iter().zip(&h.k_nets_on).map(|(id, on)| json!([rules::net_name(&d, id), on])).collect::<Vec<_>>(),
        "kKind": h.k_kind, "kKindLocked": h.k_kind_locked, "kName": h.val(|i| &i.k_name, cx),
        "show": h.show_items.iter().map(|u| json!([u.label, u.text])).collect::<Vec<_>>(),
        "showWarn": h.show_warn, "qr": h.qr.is_some(), "qrN": h.qr.as_ref().map(|q| q.1).unwrap_or(0.0),
    })
}
