//! Homepool in the browser drawer (app/vendor/crundi-browser-drawer.js
//! "Homepool (the PC browser key)"): the server's /api/homepool/browser for
//! this device (the key, its networks and their exits, the networks there
//! are, the chats and projects approved), the Network menu's Homepool group
//! and its note, a tab's exit in its tooltip and badge, and the panel
//! "Homepool for this browser" (making the PC browser key, its networks,
//! the server fallback, switching it off, removing an approval).
//!
//! Asked again when the Network menu opens and every 15 s while a tab of
//! the open project is on a homepool network or the panel is open (hpPoll);
//! nothing runs otherwise.

use crate::{ctx, drawer};
use serde_json::{json, Value};
use std::{cell::RefCell, time::Duration};

#[derive(Default)]
struct Hp {
    /// The server's answer (None: no homepool on this server, or not asked yet).
    data: Option<Value>,
    asked: bool,
    /// The panel's checks: (networks, fallback to the server, the key on).
    picks: Option<Picks>,
    polling: bool,
}

#[derive(Clone, Default, PartialEq, Debug)]
pub struct Picks { pub nets: Vec<String>, pub fallback: bool, pub on: bool }

thread_local! { static HP: RefCell<Hp> = RefCell::new(Hp::default()); }
fn with<R>(f: impl FnOnce(&mut Hp) -> R) -> R { HP.with(|h| f(&mut h.borrow_mut())) }
fn data() -> Option<Value> { with(|h| h.data.clone()) }

/// Ask the server again (the menu opened, a tab moved to a homepool network, the poll).
pub fn refresh() {
    let dev = crate::host::device_id();
    let path = format!("/api/homepool/browser{}", if dev.is_empty() { String::new() } else { format!("?device={}", ctx::enc(&dev)) });
    ctx::call(move |api| api.get(&path), |r| {
        let d = match r { Ok(d) if ctx::truthy(&d["ok"]) => Some(d), _ => None };
        let changed = with(|h| { h.asked = true; let c = h.data != d; h.data = d; c });
        if changed {
            // The panel's checks follow the key (showHp draws them from it).
            if drawer::with(|d| d.panel == "homepool") { reset_picks(); }
            drawer::render();
        }
    });
    poll();
}

/// The poll (hpPoll): every 15 s while needed.
fn poll() {
    if with(|h| h.polling) || !needed() { return; }
    with(|h| h.polling = true);
    crundi_webview::later(Duration::from_secs(15), || {
        with(|h| h.polling = false);
        if needed() { refresh(); }
    });
}

fn needed() -> bool {
    drawer::with(|d| d.open && (d.panel == "homepool" || d.projects.get(&ctx::project()).is_some_and(|p| p.tabs.iter().any(|t| t.network.starts_with("hp:")))))
}

/// One homepool network by id, from the key's list.
fn net(id: &str) -> Option<Value> {
    data()?.get("networks")?.as_array()?.iter().find(|n| ctx::s(n, "id") == id).cloned()
}

pub fn name(id: &str) -> Option<String> { net(id).map(|n| ctx::s(&n, "name")) }
pub fn available() -> bool { data().is_some() }
pub fn has_key() -> bool { data().is_some_and(|d| d.get("key").is_some_and(|k| !k.is_null())) }
pub fn exit_up(network: &str) -> bool { network.strip_prefix("hp:").and_then(net).is_some_and(|n| ctx::truthy(&n["exit"])) }

/// The networks the menu offers: the key's, not those this PC is a node of.
pub fn offered() -> Vec<(String, String, bool)> {
    if !has_key() { return vec![]; }
    data().and_then(|d| d["networks"].as_array().cloned()).unwrap_or_default().iter()
        .filter(|n| !ctx::truthy(&n["selfNode"]))
        .map(|n| (ctx::s(n, "id"), ctx::s(n, "name"), ctx::truthy(&n["exit"]))).collect()
}

/// Where a homepool tab leaves from (exitText).
pub fn exit_text(network: &str) -> String {
    let id = network.strip_prefix("hp:").unwrap_or(network);
    if !with(|h| h.asked) || data().is_none() { return "checking\u{2026}".into(); }
    if !has_key() { return "homepool is not set up for this browser".into(); }
    let Some(h) = net(id) else { return "the PC browser key does not allow this network".into() };
    if ctx::truthy(&h["selfNode"]) { return "this PC is a node of this network".into(); }
    let e = &h["exit"];
    if !ctx::truthy(e) {
        let r = ctx::s(&h, "refusal");
        return format!("No exit is up{}", if r.is_empty() { String::new() } else { format!(" ({r})") });
    }
    let ip = ctx::s(e, "publicIp");
    format!("via {}{}{}", ctx::s(e, "name"), if ip.is_empty() { String::new() } else { format!(" \u{00b7} {ip}") }, if ctx::truthy(&e["fallback"]) { " (server, fallback)" } else { "" })
}

// ─── The panel ───

/// The panel opened: the checks from the key (or every network for a new key), and the server asked.
pub fn opened() { reset_picks(); refresh(); }

fn reset_picks() {
    let d = data();
    let picks = d.as_ref().map(|d| {
        let avail = available_nets(d);
        match d.get("key").filter(|k| !k.is_null()) {
            None => Picks { nets: avail.iter().filter(|n| !n.2).map(|n| n.0.clone()).collect(), fallback: false, on: true },
            Some(k) => Picks {
                nets: k["networks"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
                fallback: ctx::truthy(&k["fallbackToServer"]),
                on: k.get("enabled") != Some(&Value::Bool(false)),
            },
        }
    });
    with(|h| h.picks = picks);
}

/// The networks there are (not the server's): (id, name, this PC is a node of it).
fn available_nets(d: &Value) -> Vec<(String, String, bool)> {
    d["available"].as_array().cloned().unwrap_or_default().iter().filter(|n| ctx::s(n, "id") != "server")
        .map(|n| (ctx::s(n, "id"), ctx::s(n, "name"), ctx::truthy(&n["selfNode"]))).collect()
}

/// What the panel draws.
#[derive(Clone, Default, PartialEq)]
pub struct PanelCopy {
    pub available: bool,
    pub has_key: bool,
    pub refusal: String,
    /// (id, name, this PC is a node of it, checked).
    pub nets: Vec<(String, String, bool, bool)>,
    /// The key's networks now: (name, exit text, an exit is up).
    pub exits: Vec<(String, String, bool)>,
    /// Approvals: (chat id or "", project or "", label, the project under a chat's title).
    pub approvals: Vec<(String, String, String, String)>,
    pub fallback: bool,
    pub on: bool,
}

pub fn panel() -> PanelCopy {
    let Some(d) = data() else { return PanelCopy::default() };
    let p = with(|h| h.picks.clone()).unwrap_or_default();
    let key = has_key();
    let nets = available_nets(&d).into_iter().map(|(id, name, me)| { let on = p.nets.contains(&id) && !me; (id, name, me, on) }).collect();
    let exits = d["networks"].as_array().cloned().unwrap_or_default().iter()
        .map(|n| (ctx::s(n, "name"), exit_text(&format!("hp:{}", ctx::s(n, "id"))), ctx::truthy(&n["exit"]))).collect();
    let mut approvals: Vec<(String, String, String, String)> = d["chats"].as_array().cloned().unwrap_or_default().iter().map(|c| {
        let id = ctx::s(c, "id");
        let t = ctx::s(c, "title");
        (id.clone(), String::new(), if t.is_empty() { format!("Chat {}", &id[..id.len().min(8)]) } else { t }, ctx::s(c, "project"))
    }).collect();
    approvals.extend(d["projects"].as_array().cloned().unwrap_or_default().iter().filter_map(|p| p.as_str()).map(|p| (String::new(), p.to_string(), format!("Every chat of {p}"), String::new())));
    PanelCopy { available: true, has_key: key, refusal: ctx::s(&d, "refusal"), nets, exits, approvals, fallback: p.fallback, on: p.on }
}

pub fn toggle_net(id: &str) {
    with(|h| if let Some(p) = h.picks.as_mut() { if let Some(i) = p.nets.iter().position(|x| x == id) { p.nets.remove(i); } else { p.nets.push(id.to_string()); } });
    drawer::render();
}
pub fn toggle_fallback() { with(|h| if let Some(p) = h.picks.as_mut() { p.fallback = !p.fallback; }); drawer::render(); }
pub fn toggle_on() { with(|h| if let Some(p) = h.picks.as_mut() { p.on = !p.on; }); drawer::render(); }

fn act(body: Value) {
    ctx::call(move |api| api.post("/api/homepool", &body), |r| match r {
        Ok(d) if ctx::truthy(&d["ok"]) => { ctx::toast("Saved"); refresh(); }
        Ok(d) => ctx::toast_error(&Some(ctx::s(&d, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "failed".into())),
        Err(e) => ctx::toast_error(&e),
    });
}

/// "Make the PC browser key" / "Save".
pub fn save() {
    let Some(p) = with(|h| h.picks.clone()) else { return };
    // Only networks this PC is not a node of.
    let me: Vec<String> = data().map(|d| available_nets(&d).into_iter().filter(|n| n.2).map(|n| n.0).collect()).unwrap_or_default();
    let nets: Vec<String> = p.nets.iter().filter(|n| !me.contains(n)).cloned().collect();
    if nets.is_empty() { return ctx::toast_error("Choose at least one network"); }
    let key_id = data().and_then(|d| d.get("key").map(|k| ctx::s(k, "id"))).unwrap_or_default();
    if has_key() {
        act(json!({ "action": "key-update", "id": key_id, "key": { "networks": nets, "fallbackToServer": p.fallback, "enabled": p.on } }));
    } else {
        act(json!({ "action": "key-add", "key": { "kind": "browser", "name": "PC browser", "networks": nets, "fallbackToServer": p.fallback, "slots": 2 } }));
    }
}

/// Remove a chat's or a project's approval.
pub fn revoke(chat: &str, project: &str) {
    if !chat.is_empty() { act(json!({ "action": "browser-revoke", "chatId": chat })); } else { act(json!({ "action": "browser-revoke", "project": project })); }
}

pub fn props() -> Value {
    let p = panel();
    json!({ "available": p.available, "key": p.has_key, "offered": offered().iter().map(|o| json!({ "id": o.0, "name": o.1, "exit": o.2 })).collect::<Vec<_>>(),
        "nets": p.nets.iter().map(|n| json!({ "id": n.0, "name": n.1, "self": n.2, "on": n.3 })).collect::<Vec<_>>(), "fallback": p.fallback, "on": p.on, "polling": with(|h| h.polling) })
}
