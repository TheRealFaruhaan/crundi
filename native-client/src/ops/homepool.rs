//! Ops area: Homepool (app/vendor/crundi-homepool.js; loadHomepoolTab and
//! homepoolFromHash in src/webapp-html.js): networks and their nodes, access
//! keys, the chats' requests, the activity log, the summary, and the
//! dialogs (add a node, give or change access, a node's code and a key's
//! secret shown once). GET /api/homepool every 3s while the tab is on show
//! (not while a dialog is open), POST /api/homepool {action} for changes.
//! UI: ui/ops/homepool.slint (global OpsHomepool).
//!
//! The web's "This PC" card (the desktop app lending its connection) is the
//! desktop area's homepool node; it is not drawn here (see PLAN.md).

use crate::{areas::AreaEvent, kit, MainWindow, OpsHomepool, OpsHpCheck, OpsHpKey, OpsHpLog, OpsHpNet, OpsHpNode, OpsHpReq, OpsHpStat, OpsHpTab, OpsHpUse};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::{cell::RefCell, rc::Rc, time::Duration};

/// What the open dialog is for.
#[derive(Clone, Debug, Default, PartialEq)]
enum Dlg {
    #[default]
    None,
    Node,
    /// Give access (None), change a key (Some(id)), or approve a request with changes.
    Key { edit: Option<String>, request: Option<String> },
    Show,
}

#[derive(Default)]
struct St {
    shown: bool,
    data: Option<Value>,
    error: String,
    tab: String,
    open_key: String,
    dlg: Dlg,
    /// The dialogs' network ids, in their selects' order.
    n_net_ids: Vec<String>,
    k_net_ids: Vec<String>,
    k_nets_on: Vec<bool>,
    k_project_ids: Vec<String>,
    copied: String,
}

thread_local! {
    static ST: RefCell<St> = RefCell::new(St { tab: "networks".into(), ..Default::default() });
    static TIMER: Timer = Timer::default();
    static TABS: Rc<VecModel<OpsHpTab>> = kit::model();
    static SUM: Rc<VecModel<OpsHpStat>> = kit::model();
    static NETS: Rc<VecModel<OpsHpNet>> = kit::model();
    static LOOSE: Rc<VecModel<OpsHpNode>> = kit::model();
    static MOVE: Rc<VecModel<SharedString>> = kit::model();
    static KEYS: Rc<VecModel<OpsHpKey>> = kit::model();
    static REQS: Rc<VecModel<OpsHpReq>> = kit::model();
    static LOG: Rc<VecModel<OpsHpLog>> = kit::model();
}

fn st<R>(f: impl FnOnce(&mut St) -> R) -> R { ST.with(|s| f(&mut s.borrow_mut())) }
fn g(w: &MainWindow) -> OpsHomepool<'_> { w.global::<OpsHomepool>() }
fn with_g(f: impl FnOnce(&OpsHomepool)) { if let Some(w) = kit::ui() { f(&g(&w)); } }
fn ss(v: &Value, k: &str) -> String { kit::s(v, k) }
fn arr<'a>(v: &'a Value, k: &str) -> Vec<&'a Value> { v.get(k).and_then(|x| x.as_array()).map(|a| a.iter().collect()).unwrap_or_default() }
fn t(v: &Value, k: &str) -> bool { kit::truthy(v.get(k).unwrap_or(&Value::Null)) }
fn vm<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> { ModelRc::new(VecModel::from(v)) }
fn now_ms() -> f64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0) }

// ─── Pure rules (the web's helpers) ───

/// bytes(): "512 B", "12 KB", "3.4 MB", "1.25 GB".
pub fn bytes(b: f64) -> String {
    if b >= 1073741824.0 { format!("{:.2} GB", b / 1073741824.0) }
    else if b >= 1048576.0 { format!("{:.1} MB", b / 1048576.0) }
    else if b >= 1024.0 { format!("{} KB", (b / 1024.0).round() as i64) }
    else { format!("{} B", b as i64) }
}

/// dur(): "2d 3h", "4h 5m", "12m", "30s".
pub fn dur(ms: f64) -> String {
    let s = (ms / 1000.0).floor().max(0.0) as i64;
    if s >= 86400 { format!("{}d {}h", s / 86400, (s % 86400) / 3600) }
    else if s >= 3600 { format!("{}h {}m", s / 3600, (s % 3600) / 60) }
    else if s >= 60 { format!("{}m", s / 60) }
    else { format!("{s}s") }
}

/// ago(): "<dur> ago", or "never".
pub fn ago_at(now: f64, t: f64) -> String { if t > 0.0 { format!("{} ago", dur(now - t)) } else { "never".into() } }

/// day(): a local date and time.
pub fn day(ms: f64) -> String {
    chrono::DateTime::from_timestamp_millis(ms as i64).map(|d| d.with_timezone(&chrono::Local).format("%-d/%-m/%Y, %H:%M:%S").to_string()).unwrap_or_default()
}

fn kind_label(k: &str) -> &str { match k { "chat" => "Chat", "project" => "Project", "software" => "Software", x => x } }

/// The log's words for an event (LOG).
pub fn log_what(w: &str) -> String {
    match w {
        "network-added" => "added network", "network-renamed" => "renamed network", "network-removed" => "removed network",
        "node-added" => "added node", "node-moved" => "moved node", "node-enabled" => "switched node on", "node-disabled" => "switched node off",
        "node-key-reissued" => "gave node a new key", "node-removed" => "removed node", "node-up" => "node connected", "node-down" => "node disconnected",
        "key-added" => "gave access", "key-changed" => "changed access", "key-reissued" => "new secret for", "key-revoked" => "revoked access",
        "request" => "a chat asked for access", "request-approved" => "approved request", "request-refused" => "refused request",
        "refused-login" => "refused a login", "settings-changed" => "changed settings",
        x => x,
    }.to_string()
}

/// whoOf(): who a key is for.
pub fn who_of(k: &Value) -> String {
    match ss(k, "kind").as_str() {
        "chat" => { let p = ss(k, "project"); format!("chat {}{}", ss(k, "chatId").chars().take(8).collect::<String>(), if p.is_empty() { String::new() } else { format!(" in {p}") }) }
        "project" => format!("every chat of {}", ss(k, "project")),
        _ => "software".into(),
    }
}

/// A local "YYYY-MM-DD HH:MM" (or with a T) as ms since 1970; None when empty or unreadable.
pub fn parse_local(s: &str) -> Option<f64> {
    let s = s.trim().replace('T', " ");
    if s.is_empty() { return None; }
    let n = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M").ok()?;
    use chrono::TimeZone;
    chrono::Local.from_local_datetime(&n).single().map(|d| d.timestamp_millis() as f64)
}

pub fn show_local(ms: f64) -> String {
    chrono::DateTime::from_timestamp_millis(ms as i64).map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default()
}

/// The QR's path and its size from the server's SVG (src/qr.js toSvg).
pub fn qr_path(svg: &str) -> Option<(String, f32)> {
    let vb = svg.split("viewBox=\"").nth(1)?.split('"').next()?;
    let n: f32 = vb.split_whitespace().nth(2)?.parse().ok()?;
    let d = svg.split(" d=\"").nth(1)?.split('"').next()?;
    if d.is_empty() { return None; }
    Some((d.to_string(), n))
}

// ─── The registry ───

pub fn init(w: &MainWindow) {
    let o = g(w);
    o.set_tabs(TABS.with(kit::rc));
    o.set_sum(SUM.with(kit::rc));
    o.set_nets(NETS.with(kit::rc));
    o.set_loose(LOOSE.with(kit::rc));
    o.set_move_opts(MOVE.with(kit::rc));
    o.set_keys(KEYS.with(kit::rc));
    o.set_reqs(REQS.with(kit::rc));
    o.set_log(LOG.with(kit::rc));
    o.on_set_tab(|k| { st(|s| s.tab = k.to_string()); render(); });
    o.on_act(|a, id| action(a.as_str(), id.as_str()));
    o.on_node_move(|id, i| {
        let net = if i <= 0 { String::new() } else { home_nets().get(i as usize - 1).map(|n| ss(n, "id")).unwrap_or_default() };
        act(json!({ "action": "node-update", "id": id.as_str(), "network": net }), "Moved", |_| {});
    });
    o.on_toggle(|kind, id, on| match kind.as_str() {
        "pool" => act(json!({ "action": "settings", "settings": { "poolBridge": { "enabled": on } } }), if on { "Standalone homepool on" } else { "Standalone homepool off" }, |_| {}),
        "node" => act(json!({ "action": "node-update", "id": id.as_str(), "enabled": on }), "", |_| {}),
        _ => act(json!({ "action": "key-update", "id": id.as_str(), "key": { "enabled": on } }), if on { "Switched on" } else { "Switched off: its connections are cut" }, |_| {}),
    });
    o.on_copy(|t| copy(t.as_str()));
    o.on_key_net(|i, on| {
        st(|s| if let Some(x) = s.k_nets_on.get_mut(i as usize) { *x = on; });
        render_key_nets();
    });
    o.on_dlg_ok(dlg_ok);
    o.on_dlg_close(close_dlg);
}

pub fn signed_in() {}

pub fn signed_out() {
    TIMER.with(|t| t.stop());
    st(|s| *s = St { tab: "networks".into(), ..Default::default() });
    close_dlg();
    render();
}

pub fn project(_alias: &str) {}

pub fn tab_shown(tab: &str) {
    let was = st(|s| s.shown);
    if tab == "homepool" && !was {
        st(|s| s.shown = true);
        load();
        // Live while open; a dialog being filled in is left alone.
        TIMER.with(|t| t.start(TimerMode::Repeated, Duration::from_millis(3000), || { if st(|s| s.dlg == Dlg::None) { load(); } }));
    } else if tab != "homepool" && was {
        st(|s| s.shown = false);
        TIMER.with(|t| t.stop());
    }
}

/// The notification's link (#homepool): the web's homepoolFromHash opens the tab.
pub fn event(_e: &AreaEvent) {}

// ─── Data ───

fn load() {
    kit::call(|api| api.get("/api/homepool"), |r| {
        match r {
            Err(e) if e == "Not supported by this server" => st(|s| s.error = "This server does not support homepool yet. Update the server to use it.".into()),
            Err(e) if e.starts_with("Cannot reach") => st(|s| s.error = "Could not reach the server.".into()),
            Err(e) => st(|s| s.error = if e.is_empty() { "Could not load homepool.".into() } else { e }),
            Ok(d) if d.get("ok") == Some(&Value::Bool(false)) => st(|s| s.error = { let e = ss(&d, "error"); if e.is_empty() { "Could not load homepool.".into() } else { e } }),
            Ok(d) => st(|s| { s.error.clear(); s.data = Some(d); }),
        }
        render();
    });
}

/// POST /api/homepool: the answer's overview redraws; `ok` is toasted.
fn act(payload: Value, ok: &str, then: impl FnOnce(Option<Value>) + Send + 'static) {
    let ok = ok.to_string();
    kit::call(move |api| api.post("/api/homepool", &payload), move |r| {
        let r = match r {
            Ok(d) if d.get("ok") == Some(&Value::Bool(false)) => Err(ss(&d, "error")),
            Err(e) if e.starts_with("Cannot reach") => Err("Could not reach the server".into()),
            x => x,
        };
        match r {
            Err(e) => { kit::toast_error(if e.is_empty() { "That did not work" } else { &e }); then(None); }
            Ok(d) => {
                if let Some(ov) = d.get("overview").filter(|x| x.is_object()) {
                    let mut ov = ov.clone();
                    ov["ok"] = json!(true);
                    st(|s| s.data = Some(ov));
                    render();
                }
                if !ok.is_empty() { kit::toast_ok(&ok); }
                then(Some(d));
            }
        }
    });
}

fn data() -> Value { st(|s| s.data.clone().unwrap_or_default()) }

fn net_name(d: &Value, id: &str) -> String {
    if id.is_empty() { return "No network".into(); }
    arr(d, "networks").into_iter().find(|n| ss(n, "id") == id).map(|n| ss(n, "name")).unwrap_or_else(|| id.to_string())
}

fn home_nets() -> Vec<Value> {
    arr(&data(), "networks").into_iter().filter(|n| { let i = ss(n, "id"); i != "server" && i != "pool" }).cloned().collect()
}

fn copy(text: &str) {
    st(|s| s.copied = text.to_string());
    let done = arboard::Clipboard::new().ok().and_then(|mut cb| cb.set_text(text.to_string()).ok().map(|_| cb));
    match done {
        Some(cb) => { crate::app::keep_clipboard(cb); kit::toast_ok("Copied"); }
        None => kit::toast_error("Copy failed"),
    }

}

// ─── Actions ───

fn action(a: &str, id: &str) {
    let d = data();
    let find = |k: &str| arr(&d, k).into_iter().find(|x| ss(x, "id") == id).cloned().unwrap_or_default();
    let (node, key, req) = (find("nodes"), find("keys"), find("requests"));
    let id = id.to_string();
    match a {
        "net-add" => kit::prompt("Add a network", "Name (where it is: Home, Ali’s place, Office)", "", "", "Save", false, |v| {
            if let Some(v) = v.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) { act(json!({ "action": "network-add", "name": v }), "Network added", |_| {}); }
        }),
        "net-rename" => kit::prompt("Rename network", "Name", &net_name(&d, &id), "", "Save", false, move |v| {
            if let Some(v) = v.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) { act(json!({ "action": "network-rename", "id": id, "name": v }), "", |_| {}); }
        }),
        "net-remove" => kit::confirm(kit::Confirm { title: &format!("Remove \"{}\"?", net_name(&d, &id)), message: "Its nodes stay, but carry nothing until you put them in another network. Keys lose this network.", ok: "Remove", danger: true }, move |ok| {
            if ok { act(json!({ "action": "network-remove", "id": id }), "Removed", |_| {}); }
        }),
        "node-add" => open_node_dialog(),
        "node-rename" => kit::prompt("Rename node", "Name", &ss(&node, "name"), "", "Save", false, move |v| {
            if let Some(v) = v.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) { act(json!({ "action": "node-update", "id": id, "name": v }), "", |_| {}); }
        }),
        "node-reissue" => {
            let name = ss(&node, "name");
            kit::confirm(kit::Confirm { title: &format!("New key for \"{name}\"?"), message: "The old key stops working now and the node disconnects. Set it up again with the new code.", ok: "New key", danger: false }, move |ok| {
                if ok { act(json!({ "action": "node-reissue", "id": id }), "", move |r| { if let Some(r) = r { show_node_setup(&r["node"], &format!("New key for \"{name}\"")); } }); }
            })
        }
        "node-remove" => kit::confirm(kit::Confirm { title: &format!("Remove \"{}\"?", ss(&node, "name")), message: "It disconnects now and its key stops working for good.", ok: "Remove", danger: true }, move |ok| {
            if ok { act(json!({ "action": "node-remove", "id": id }), "Removed", |_| {}); }
        }),
        "key-add" => open_key_dialog(None, None),
        "key-edit" => open_key_dialog(Some(key), None),
        "key-use" => { st(|s| s.open_key = if s.open_key == id { String::new() } else { id.clone() }); render(); }
        "key-reissue" => {
            let name = ss(&key, "name");
            kit::confirm(kit::Confirm { title: &format!("New secret for \"{name}\"?"), message: "The old one stops working now.", ok: "New secret", danger: false }, move |ok| {
                if ok { act(json!({ "action": "key-reissue", "id": id }), "", move |r| { if let Some(r) = r { show_secret(&r, &name); } }); }
            })
        }
        "key-remove" => kit::confirm(kit::Confirm { title: &format!("Revoke \"{}\"?", ss(&key, "name")), message: "Its open connections end now and it can never be used again.", ok: "Revoke", danger: true }, move |ok| {
            if ok { act(json!({ "action": "key-remove", "id": id }), "Revoked", |_| {}); }
        }),
        "req-approve" => act(json!({ "action": "request-decide", "id": id, "approve": true }), "Approved", |_| {}),
        "req-refuse" => act(json!({ "action": "request-decide", "id": id, "approve": false }), "Refused", |_| {}),
        "req-change" => {
            if req.is_null() { return; }
            let k = json!({ "kind": ss(&req, "kind"), "name": ss(&req, "name"), "project": ss(&req, "project"), "chatId": ss(&req, "chatId"), "networks": req.get("networks").cloned().unwrap_or(json!([])), "slots": 8 });
            open_key_dialog(Some(k), Some(id));
        }
        _ => {}
    }
}

// ─── Dialogs ───

fn set_dlg(d: Dlg, title: &str, ok: &str) {
    let kind = match &d { Dlg::None => "", Dlg::Node => "node", Dlg::Key { .. } => "key", Dlg::Show => "show" };
    st(|s| s.dlg = d);
    with_g(|o| {
        o.set_dlg(kind.into());
        o.set_dlg_title(title.into());
        o.set_dlg_label(ok.into());
        o.set_modal(!kind.is_empty());
    });
}

fn close_dlg() { set_dlg(Dlg::None, "", ""); }

fn open_node_dialog() {
    let nets = home_nets();
    let sel = nets.iter().position(|n| ss(n, "id") == "home").map(|i| i as i32 + 1).unwrap_or(0);
    let mut labels = vec![SharedString::from("No network yet")];
    labels.extend(nets.iter().map(|n| SharedString::from(ss(n, "name"))));
    st(|s| s.n_net_ids = std::iter::once(String::new()).chain(nets.iter().map(|n| ss(n, "id"))).collect());
    with_g(|o| { o.set_n_name("".into()); o.set_n_net(sel); o.set_n_nets(vm(labels)); });
    set_dlg(Dlg::Node, "Add a node", "Make node");
}

fn open_key_dialog(k: Option<Value>, request: Option<String>) {
    let edit = k.as_ref().filter(|k| !ss(k, "id").is_empty() && request.is_none()).map(|k| ss(k, "id"));
    let k = k.unwrap_or_else(|| json!({ "kind": "software", "networks": ["server"], "slots": 8 }));
    let kinds = ["software", "project", "chat"];
    let kind = kinds.iter().position(|x| *x == ss(&k, "kind")).unwrap_or(0) as i32;
    let projects = kit::projects();
    let mut plabels = vec![SharedString::from("—")];
    plabels.extend(projects.iter().map(|p| { let n = ss(p, "name"); SharedString::from(if n.is_empty() { ss(p, "alias") } else { n }) }));
    let pids: Vec<String> = std::iter::once(String::new()).chain(projects.iter().map(|p| ss(p, "alias"))).collect();
    let psel = pids.iter().position(|a| !a.is_empty() && *a == ss(&k, "project")).unwrap_or(0) as i32;
    let nets: Vec<Value> = arr(&data(), "networks").into_iter().cloned().collect();
    let want: Vec<String> = arr(&k, "networks").into_iter().map(|x| x.as_str().unwrap_or("").to_string()).collect();
    st(|s| {
        s.k_net_ids = nets.iter().map(|n| ss(n, "id")).collect();
        s.k_nets_on = nets.iter().map(|n| want.contains(&ss(n, "id"))).collect();
        s.k_project_ids = pids;
    });
    let slots = k.get("slots").filter(|x| !x.is_null()).map(|x| x.to_string()).unwrap_or_else(|| "8".into());
    let from = { let a: Vec<String> = arr(&k, "allowFrom").into_iter().map(|x| x.as_str().unwrap_or("").to_string()).collect(); if a.is_empty() { "homepool".to_string() } else { a.join(", ") } };
    with_g(|o| {
        o.set_k_kind(kind);
        o.set_k_kind_locked(edit.is_some());
        o.set_k_name(ss(&k, "name").into());
        o.set_k_projects(vm(plabels));
        o.set_k_project(psel);
        o.set_k_project_locked(edit.is_some() || request.is_some());
        o.set_k_chat(ss(&k, "chatId").into());
        o.set_k_fb(t(&k, "fallbackToServer"));
        o.set_k_exp(k.get("expiresAt").and_then(|x| x.as_f64()).map(show_local).unwrap_or_default().into());
        o.set_k_cap(format!("{}", kit::n(&k, "dailyCapMb") as i64).into());
        o.set_k_slots(slots.into());
        o.set_k_block(t(&k, "portBlock"));
        o.set_k_from(from.into());
    });
    render_key_nets();
    let (title, ok) = if edit.is_some() { (format!("Change access \"{}\"", ss(&k, "name")), "Save") } else if request.is_some() { ("Approve with changes".to_string(), "Approve") } else { ("Give access".to_string(), "Give access") };
    set_dlg(Dlg::Key { edit, request }, &title, ok);
}

fn render_key_nets() {
    let d = data();
    let (ids, on) = st(|s| (s.k_net_ids.clone(), s.k_nets_on.clone()));
    let rows: Vec<OpsHpCheck> = ids.iter().zip(on).map(|(id, on)| OpsHpCheck { label: net_name(&d, id).into(), on }).collect();
    with_g(|o| o.set_k_nets(vm(rows)));
}

/// readKey(): the access dialog as the server takes it.
fn read_key() -> Value {
    let Some(w) = kit::ui() else { return Value::Null };
    let o = g(&w);
    let kind = ["software", "project", "chat"][o.get_k_kind().clamp(0, 2) as usize];
    let (nets, project) = st(|s| (
        s.k_net_ids.iter().zip(&s.k_nets_on).filter(|(_, on)| **on).map(|(id, _)| id.clone()).collect::<Vec<_>>(),
        s.k_project_ids.get(o.get_k_project().max(0) as usize).cloned().unwrap_or_default(),
    ));
    let mut out = json!({
        "kind": kind,
        "name": o.get_k_name().trim(),
        "project": project,
        "chatId": o.get_k_chat().trim(),
        "networks": nets,
        "fallbackToServer": o.get_k_fb(),
        "expiresAt": parse_local(&o.get_k_exp()),
        "dailyCapMb": o.get_k_cap().trim().parse::<f64>().unwrap_or(0.0),
        "slots": o.get_k_slots().trim().parse::<f64>().unwrap_or(0.0),
        "portBlock": kind == "software" && o.get_k_block(),
        "allowFrom": o.get_k_from().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>(),
    });
    if o.get_k_name().trim().is_empty() { out.as_object_mut().unwrap().remove("name"); }
    out
}

fn dlg_ok() {
    let dlg = st(|s| s.dlg.clone());
    match dlg {
        Dlg::Node => {
            let Some(w) = kit::ui() else { return };
            let o = g(&w);
            let name = o.get_n_name().trim().to_string();
            if name.is_empty() { kit::toast_error("Give the node a name"); return; }
            let net = st(|s| s.n_net_ids.get(o.get_n_net().max(0) as usize).cloned().unwrap_or_default());
            act(json!({ "action": "node-add", "name": name, "network": net }), "", move |r| {
                if let Some(r) = r { close_dlg(); show_node_setup(&r["node"], &format!("Node \"{name}\" is ready")); }
            });
        }
        Dlg::Key { edit, request } => {
            let key = read_key();
            if arr(&key, "networks").is_empty() { kit::toast_error("Pick at least one network"); return; }
            if let Some(req) = request {
                act(json!({ "action": "request-decide", "id": req, "approve": true, "changes": key }), "Approved", |r| { if r.is_some() { close_dlg(); } });
            } else if let Some(id) = edit {
                act(json!({ "action": "key-update", "id": id, "key": key }), "Saved", |r| { if r.is_some() { close_dlg(); } });
            } else {
                let name = { let n = ss(&key, "name"); if n.is_empty() { "Access".to_string() } else { n } };
                act(json!({ "action": "key-add", "key": key }), "", move |r| {
                    if let Some(r) = r {
                        close_dlg();
                        if !ss(&r, "secret").is_empty() { show_secret(&r, &name); }
                    }
                });
            }
        }
        _ => close_dlg(),
    }
}

fn use_line(label: &str, kind: &str, text: &str) -> OpsHpUse { OpsHpUse { label: label.into(), kind: kind.into(), text: text.into() } }

fn show_node_setup(node: &Value, title: &str) {
    let code = { let c = ss(node, "code"); if c.is_empty() { ss(node, "key") } else { c } };
    let mut items = vec![use_line("CODE (SCAN OR PASTE IN THE HOMEPOOL NODE APP OR THE CRUNDI APP)", "code", &code)];
    let docker = ss(node, "docker");
    if !docker.is_empty() { items.push(use_line("OR ON ANY MACHINE WITH DOCKER", "code", &docker)); }
    let (qr, n) = qr_path(&ss(node, "qr")).unwrap_or_default();
    with_g(|o| {
        o.set_show_warn("The key is shown this once. Whoever has it can lend their connection to this server as this node (and nothing else). Give it only to the person running the node.".into());
        o.set_show_items(vm(items));
        o.set_qr(qr.into());
        o.set_qr_n(n);
        o.set_show_warn2(if ss(node, "server").is_empty() { "This server has no public address set, so the code has none. Set one (TLS domain or tunnel) for nodes outside this machine." } else { "" }.into());
    });
    set_dlg(Dlg::Show, title, "");
}

fn show_secret(r: &Value, name: &str) {
    let u = r.get("use").cloned().unwrap_or_default();
    let mut items = vec![use_line("LOGIN", "code", &format!("user: {}\npassword: {}", ss(&r["key"], "id"), ss(r, "secret")))];
    if !ss(&u, "socks").is_empty() { items.push(use_line("SOCKS5", "code", &ss(&u, "socks"))); }
    if !ss(&u, "http").is_empty() { items.push(use_line("HTTP PROXY", "code", &ss(&u, "http"))); }
    if let Some(b) = u.get("block").filter(|b| b.is_object()) {
        items.push(use_line("PORT BLOCK (NO LOGIN)", "code", &arr(b, "compose").iter().map(|x| x.as_str().unwrap_or("")).collect::<Vec<_>>().join("\n")));
    }
    with_g(|o| {
        o.set_show_warn("The secret is shown this once. Keep it with the software (its .env or a Crundi secret), never in a file you commit.".into());
        o.set_show_items(vm(items));
        o.set_qr("".into());
        o.set_show_warn2("".into());
    });
    set_dlg(Dlg::Show, &format!("Access \"{name}\""), "");
}

// ─── Drawing ───

fn chip_tone(c: &str) -> &str { c }

fn node_row(d: &Value, n: &Value, now: f64) -> OpsHpNode {
    let i = n.get("info").cloned().unwrap_or_default();
    let enabled = n.get("enabled") != Some(&Value::Bool(false));
    let (up, paused) = (t(n, "up"), t(n, "paused"));
    let dot = if !enabled { "" } else if up { if paused { "paused" } else { "up" } } else { "" };
    let reason = ss(&i, "pauseReason");
    let (state, tone) = if !enabled { ("switched off".to_string(), "") } else if up {
        if paused { (format!("paused{}", if reason.is_empty() { String::new() } else { format!(": {reason}") }), "warn") } else { ("connected".into(), "ok") }
    } else { ("offline".into(), "") };
    let mut meta = Vec::new();
    let plat = { let p = ss(&i, "platform"); if p.is_empty() { ss(n, "platform") } else { p } };
    meta.push(format!("{}{}", if ss(n, "kind") == "crundi-app" { "Crundi app" } else { "Standalone node" }, if plat.is_empty() { String::new() } else { format!(" · {plat}") }));
    if up {
        if !ss(&i, "network").is_empty() { meta.push(ss(&i, "network")); }
        if !ss(&i, "publicIp").is_empty() { meta.push(format!("IP {}", ss(&i, "publicIp"))); }
        meta.push(format!("up {}", dur(now - kit::n(n, "connectedAt"))));
        meta.push(format!("{} open", kit::n(n, "streams") as i64));
    } else {
        meta.push(format!("last seen {}", ago_at(now, kit::n(n, "lastSeenAt"))));
    }
    let slots = kit::n(n, "slots") as i64;
    meta.push(format!("{slots} slot{}", if slots == 1 { "" } else { "s" }));
    meta.push(format!("{} today", bytes(n.get("usage").map(|u| kit::n(u, "bytes")).unwrap_or(0.0))));
    if kit::n(&i, "mobileCapMb") > 0.0 { meta.push(format!("mobile {} of {} MB", bytes(kit::n(&i, "mobileBytesToday")), ss(&i, "mobileCapMb"))); }
    let homes: Vec<String> = arr(d, "networks").into_iter().map(|x| ss(x, "id")).filter(|x| x != "server" && x != "pool").collect();
    let net = homes.iter().position(|x| *x == ss(n, "network")).map(|p| p as i32 + 1).unwrap_or(0);
    OpsHpNode { id: ss(n, "id").into(), name: ss(n, "name").into(), dot: dot.into(), state: state.into(), tone: chip_tone(tone).into(), meta: meta.join(" · ").into(), net, on: enabled }
}

fn render() {
    let Some(w) = kit::ui() else { return };
    let o = g(&w);
    let (data, error, tab, open_key) = st(|s| (s.data.clone(), s.error.clone(), s.tab.clone(), s.open_key.clone()));
    let Some(d) = data else {
        o.set_state(if error.is_empty() { "loading" } else { "error" }.into());
        o.set_error(error.into());
        return;
    };
    o.set_state("data".into());
    o.set_tab(tab.clone().into());
    let now = now_ms();
    let (nodes, keys, reqs) = (arr(&d, "nodes"), arr(&d, "keys"), arr(&d, "requests"));
    let pending = reqs.iter().filter(|r| ss(r, "status") == "pending").count();
    TABS.with(|m| kit::sync_rows(m, vec![
        OpsHpTab { key: "networks".into(), label: "Networks".into(), count: nodes.len().to_string().into(), alert: false },
        OpsHpTab { key: "access".into(), label: "Access".into(), count: keys.len().to_string().into(), alert: false },
        OpsHpTab { key: "requests".into(), label: "Requests".into(), count: pending.to_string().into(), alert: pending > 0 },
        OpsHpTab { key: "activity".into(), label: "Activity".into(), count: "".into(), alert: false },
    ]));
    let up = nodes.iter().filter(|n| t(n, "up") && !t(n, "paused")).count();
    let today: f64 = keys.iter().map(|k| k.get("usage").map(|u| kit::n(u, "bytes")).unwrap_or(0.0)).sum();
    let stat = |v: String, l: &str| OpsHpStat { v: v.into(), l: l.to_uppercase().into() };
    SUM.with(|m| kit::sync_rows(m, vec![
        stat(format!("{up}/{}", nodes.len()), "nodes up"),
        stat(keys.iter().filter(|k| ss(k, "refusal").is_empty()).count().to_string(), "keys in use"),
        stat(bytes(today), "data today"),
        stat(pending.to_string(), "waiting for you"),
    ]));
    // A node's network <select>: no network, then the owner's networks.
    let mut opts = vec![SharedString::from("No network")];
    opts.extend(arr(&d, "networks").into_iter().filter(|x| { let i = ss(x, "id"); i != "server" && i != "pool" }).map(|x| SharedString::from(ss(x, "name"))));
    MOVE.with(|m| kit::sync_rows(m, opts));
    let nets: Vec<OpsHpNet> = arr(&d, "networks").into_iter().map(|n| {
        let id = ss(n, "id");
        let mut net = OpsHpNet { id: id.clone().into(), name: ss(n, "name").into(), ..Default::default() };
        if id == "server" {
            net.kind = "server".into(); net.chip = "always up".into(); net.tone = "ok".into();
            net.body = "This server’s own connection. The default for every key.".into();
        } else if id == "pool" {
            let p = n.get("pool").cloned().unwrap_or_default();
            let on = n.get("enabled") != Some(&Value::Bool(false));
            net.kind = "pool".into(); net.on = on;
            let (c, tn) = if !on { ("bridge off".to_string(), "") } else if t(&p, "up") { (format!("{} node(s) up", kit::n(&p, "liveCount") as i64), "ok") } else { ("down".into(), "warn") };
            net.chip = c.into(); net.tone = tn.into();
            let ups: Vec<String> = arr(&p, "nodes").iter().map(|x| { let nm = ss(x, "name"); if nm.is_empty() { ss(x, "id") } else { nm } }).collect();
            net.body = format!("The standalone homepool (homepool-relay) as it runs today. A key on this network keeps the same slots, so the same identities keep the same home lines.{}", if ups.is_empty() { String::new() } else { format!(" Up: {}.", ups.join(", ")) }).into();
            net.err = ss(&p, "error").into();
        } else {
            let live = kit::n(n, "live") as i64;
            net.kind = "own".into(); net.chip = format!("{live} up").into(); net.tone = if live > 0 { "ok" } else { "" }.into();
            let members: Vec<OpsHpNode> = nodes.iter().filter(|x| ss(x, "network") == id).map(|x| node_row(&d, x, now)).collect();
            if members.is_empty() { net.body = "No node in this network yet. Add one, or move one here.".into(); }
            net.nodes = vm(members);
        }
        net
    }).collect();
    NETS.with(|m| kit::sync_rows(m, nets));
    let net_ids: Vec<String> = arr(&d, "networks").into_iter().map(|n| ss(n, "id")).collect();
    LOOSE.with(|m| kit::sync_rows(m, nodes.iter().filter(|x| { let n = ss(x, "network"); n.is_empty() || !net_ids.contains(&n) }).map(|x| node_row(&d, x, now)).collect()));
    // Access.
    let rows: Vec<OpsHpKey> = keys.iter().map(|k| {
        let u = k.get("usage").cloned().unwrap_or_default();
        let s = k.get("status").cloned().unwrap_or_default();
        let refusal = ss(k, "refusal");
        let (state, tone) = if !refusal.is_empty() { (refusal, "bad") } else if t(&s, "egressAvailable") {
            ((if t(&s, "fallbackInUse") { "leaving from the server (fallback)" } else { "exit up" }).to_string(), "ok")
        } else { ("no exit up".into(), "warn") };
        let kind = ss(k, "kind");
        let mut meta = vec![
            if kind == "software" { (if t(k, "portBlock") { "Software, port block" } else { "Software, own login" }).to_string() } else { format!("{}: {}", kind_label(&kind), who_of(k)) },
            format!("{} today", bytes(kit::n(&u, "bytes"))),
            format!("{} connections", kit::n(&u, "conns") as i64),
            format!("{} refused", kit::n(&u, "refused") as i64),
            format!("used {}", ago_at(now, kit::n(&u, "lastUsedAt"))),
        ];
        if kit::n(k, "open") > 0.0 { meta.push(format!("{} open now", kit::n(k, "open") as i64)); }
        if kit::n(k, "dailyCapMb") > 0.0 { meta.push(format!("cap {} MB/day", ss(k, "dailyCapMb"))); }
        if kit::n(k, "expiresAt") > 0.0 { meta.push(format!("expires {}", day(kit::n(k, "expiresAt")))); }
        if t(k, "fallbackToServer") { meta.push("falls back to the server".into()); }
        if t(k, "portBlock") { if let Some(p) = k.get("ports").filter(|p| p.is_object()) {
            meta.push(format!("ports {}-{} from {}", ss(p, "base"), ss(p, "status"), arr(k, "allowFrom").iter().map(|x| x.as_str().unwrap_or("")).collect::<Vec<_>>().join(", ")));
        } }
        let open = open_key == ss(k, "id");
        let mut uses = Vec::new();
        if let Some(us) = k.get("use").filter(|x| x.is_object() && open) {
            if kind == "software" {
                uses.push(use_line("", "quiet", &format!("Login: {}", ss(us, "login"))));
                uses.push(use_line("", "code", &ss(us, "socks")));
                uses.push(use_line("", "code", &ss(us, "http")));
                uses.push(use_line("", "quiet", &format!("Status for the software to check before it starts: {}", ss(us, "status"))));
                if !ss(us, "fromDocker").is_empty() { uses.push(use_line("", "quiet", &format!("From a container: {}", ss(us, "fromDocker")))); }
            } else {
                uses.push(use_line("", "quiet", "Chats use it through the MCP tools homepool_proxy and homepool_fetch."));
            }
            if let Some(b) = us.get("block").filter(|b| b.is_object()) {
                uses.push(use_line("", "quiet", &format!("Port block (no login; only from {}): any slot {}, slots {}. For docker compose:", arr(b, "from").iter().map(|x| x.as_str().unwrap_or("")).collect::<Vec<_>>().join(", "), ss(b, "any"), ss(b, "slots"))));
                uses.push(use_line("", "code", &arr(b, "compose").iter().map(|x| x.as_str().unwrap_or("")).collect::<Vec<_>>().join("\n")));
            }
        }
        OpsHpKey {
            id: ss(k, "id").into(), name: ss(k, "name").into(),
            nets: vm(arr(k, "networks").iter().map(|n| SharedString::from(net_name(&d, n.as_str().unwrap_or("")))).collect()),
            state: state.into(), tone: tone.into(), on: k.get("enabled") != Some(&Value::Bool(false)),
            meta: meta.join(" · ").into(), open, software: kind == "software", uses: vm(uses),
        }
    }).collect();
    KEYS.with(|m| kit::sync_rows(m, rows));
    // Requests.
    let rows: Vec<OpsHpReq> = reqs.iter().map(|r| {
        let status = ss(r, "status");
        let pend = status == "pending";
        let (name, p) = (ss(r, "name"), ss(r, "project"));
        OpsHpReq {
            id: ss(r, "id").into(),
            title: format!("{}{}", kind_label(&ss(r, "kind")), if name.is_empty() { String::new() } else { format!(" \"{name}\"") }).into(),
            nets: vm(arr(r, "networks").iter().map(|n| SharedString::from(net_name(&d, n.as_str().unwrap_or("")))).collect()),
            tone: if pend { "warn" } else if status == "approved" { "ok" } else { "bad" }.into(),
            status: status.into(), pending: pend,
            meta: format!("From chat {}{}, {}", ss(r, "chatId").chars().take(8).collect::<String>(), if p.is_empty() { String::new() } else { format!(" in {p}") }, ago_at(now, kit::n(r, "createdAt"))).into(),
            reason: ss(r, "reason").into(),
        }
    }).collect();
    REQS.with(|m| kit::sync_rows(m, rows));
    // Activity.
    let names = |id: &str| nodes.iter().chain(keys.iter()).find(|x| ss(x, "id") == id).map(|x| ss(x, "name")).unwrap_or_else(|| id.to_string());
    let lines: Vec<OpsHpLog> = arr(&d, "log").iter().map(|e| {
        let what = ss(e, "what");
        let mut subj = if !ss(e, "node").is_empty() { names(&ss(e, "node")) } else if !ss(e, "key").is_empty() { names(&ss(e, "key")) }
            else if !ss(e, "network").is_empty() { net_name(&d, &ss(e, "network")) } else if !ss(e, "request").is_empty() { ss(e, "request") }
            else if !ss(e, "user").is_empty() { format!("{}{}", ss(e, "user"), { let f = ss(e, "from"); if f.is_empty() { String::new() } else { format!(" from {f}") } }) } else { String::new() };
        if what == "node-moved" { subj.push_str(&format!(" → {}", net_name(&d, &ss(e, "to")))); }
        OpsHpLog { at: day(kit::n(e, "at")).into(), what: log_what(&what).into(), subj: subj.into() }
    }).collect();
    LOG.with(|m| kit::sync_rows(m, lines));
}

// ─── The test driver ───

fn arg(a: &[String], i: usize) -> &str { a.get(i).map(|s| s.as_str()).unwrap_or("") }

/// The driver's "ops:<name>" actions this screen knows (None: not mine).
pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    if !name.starts_with("homepool-") { return None; }
    let w = kit::ui()?;
    let o = g(&w);
    let ok = Some(json!({ "ok": true }));
    match name {
        "homepool-load" => { load(); ok }
        "homepool-tab" => { o.invoke_set_tab(arg(args, 0).into()); ok }
        "homepool-act" => { o.invoke_act(arg(args, 0).into(), arg(args, 1).into()); ok }
        "homepool-move" => { o.invoke_node_move(arg(args, 0).into(), arg(args, 1).parse().unwrap_or(0)); ok }
        "homepool-toggle" => { o.invoke_toggle(arg(args, 0).into(), arg(args, 1).into(), arg(args, 2) == "true"); ok }
        "homepool-copy" => { o.invoke_copy(arg(args, 0).into()); Some(json!({ "ok": true, "text": st(|s| s.copied.clone()) })) }
        // The node dialog: name, network index.
        "homepool-node-fill" => { o.set_n_name(arg(args, 0).into()); o.set_n_net(arg(args, 1).parse().unwrap_or(0)); ok }
        // The access dialog: kind index, name, network ids (comma), fallback, cap, slots, expires, project index, chat id.
        "homepool-key-fill" => {
            if !o.get_k_kind_locked() { o.set_k_kind(arg(args, 0).parse().unwrap_or(0)); }
            o.set_k_name(arg(args, 1).into());
            let want: Vec<&str> = arg(args, 2).split(',').filter(|x| !x.is_empty()).collect();
            st(|s| s.k_nets_on = s.k_net_ids.iter().map(|id| want.contains(&id.as_str())).collect());
            render_key_nets();
            o.set_k_fb(arg(args, 3) == "true");
            o.set_k_cap(arg(args, 4).into());
            o.set_k_slots(arg(args, 5).into());
            o.set_k_exp(arg(args, 6).into());
            if !o.get_k_project_locked() { o.set_k_project(arg(args, 7).parse().unwrap_or(0)); o.set_k_chat(arg(args, 8).into()); }
            ok
        }
        "homepool-dlg-ok" => { o.invoke_dlg_ok(); ok }
        "homepool-dlg-close" => { o.invoke_dlg_close(); ok }
        _ => None,
    }
}

/// This screen's part of the area's props (under "homepool").
pub fn props() -> Value {
    let Some(w) = kit::ui() else { return json!({}) };
    let o = g(&w);
    let shown = st(|s| s.shown);
    let node = |n: &OpsHpNode| json!({ "id": n.id.as_str(), "name": n.name.as_str(), "state": n.state.as_str(), "meta": n.meta.as_str(), "net": n.net, "on": n.on });
    json!({
        "shown": shown,
        "state": o.get_state().as_str(), "error": o.get_error().as_str(), "tab": o.get_tab().as_str(),
        "tabs": o.get_tabs().iter().map(|t| json!([t.label.as_str(), t.count.as_str(), t.alert])).collect::<Vec<_>>(),
        "sum": o.get_sum().iter().map(|s| json!([s.v.as_str(), s.l.as_str()])).collect::<Vec<_>>(),
        "nets": o.get_nets().iter().map(|n| json!({ "id": n.id.as_str(), "name": n.name.as_str(), "kind": n.kind.as_str(), "chip": n.chip.as_str(), "on": n.on, "body": n.body.as_str(), "nodes": n.nodes.iter().map(|x| node(&x)).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "loose": o.get_loose().iter().map(|x| node(&x)).collect::<Vec<_>>(),
        "moveOpts": o.get_move_opts().iter().map(|x| x.to_string()).collect::<Vec<_>>(),
        "keys": o.get_keys().iter().map(|k| json!({ "id": k.id.as_str(), "name": k.name.as_str(), "state": k.state.as_str(), "on": k.on, "meta": k.meta.as_str(), "open": k.open, "nets": k.nets.iter().map(|x| x.to_string()).collect::<Vec<_>>(), "uses": k.uses.iter().map(|u| u.text.to_string()).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "reqs": o.get_reqs().iter().map(|r| json!({ "id": r.id.as_str(), "title": r.title.as_str(), "status": r.status.as_str(), "pending": r.pending, "meta": r.meta.as_str() })).collect::<Vec<_>>(),
        "log": o.get_log().iter().map(|e| format!("{} {}", e.what, e.subj)).collect::<Vec<_>>(),
        "dlg": o.get_dlg().as_str(), "dlgTitle": o.get_dlg_title().as_str(), "dlgOk": o.get_dlg_label().as_str(), "modal": o.get_modal(),
        "kNets": o.get_k_nets().iter().map(|c| json!([c.label.as_str(), c.on])).collect::<Vec<_>>(),
        "kKind": o.get_k_kind(), "kKindLocked": o.get_k_kind_locked(), "kName": o.get_k_name().as_str(),
        "show": o.get_show_items().iter().map(|u| json!([u.label.as_str(), u.text.as_str()])).collect::<Vec<_>>(),
        "showWarn": o.get_show_warn().as_str(), "qr": !o.get_qr().is_empty(), "qrN": o.get_qr_n(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(bytes(500.0), "500 B");
        assert_eq!(bytes(2048.0), "2 KB");
        assert_eq!(bytes(3.5 * 1048576.0), "3.5 MB");
        assert_eq!(bytes(2.0 * 1073741824.0), "2.00 GB");
        assert_eq!(dur(30_000.0), "30s");
        assert_eq!(dur(125_000.0), "2m");
        assert_eq!(dur(3_725_000.0), "1h 2m");
        assert_eq!(dur(90_000_000.0), "1d 1h");
        assert_eq!(ago_at(100_000.0, 0.0), "never");
        assert_eq!(ago_at(100_000.0, 40_000.0), "1m ago");
    }

    #[test]
    fn words() {
        assert_eq!(log_what("node-moved"), "moved node");
        assert_eq!(log_what("custom"), "custom");
        assert_eq!(who_of(&json!({ "kind": "chat", "chatId": "abcdef123456", "project": "demo" })), "chat abcdef12 in demo");
        assert_eq!(who_of(&json!({ "kind": "project", "project": "demo" })), "every chat of demo");
        assert_eq!(who_of(&json!({ "kind": "software" })), "software");
    }

    #[test]
    fn times_and_qr() {
        let ms = parse_local("2026-11-15 10:30").unwrap();
        assert_eq!(show_local(ms), "2026-11-15 10:30");
        assert_eq!(parse_local("2026-11-15T10:30"), Some(ms));
        assert_eq!(parse_local(""), None);
        assert_eq!(parse_local("soon"), None);
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 29 29" shape-rendering="crispEdges"><rect width="29" height="29" fill="#fff"/><path d="M4 4h7v1h-7z" fill="#000"/></svg>"##;
        assert_eq!(qr_path(svg), Some(("M4 4h7v1h-7z".into(), 29.0)));
        assert_eq!(qr_path(""), None);
    }
}
