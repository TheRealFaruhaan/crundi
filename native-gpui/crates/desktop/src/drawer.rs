//! The browser drawer (app/vendor/crundi-browser-drawer.js and
//! app/browser-drawer.js; spec specs/desktop/drawer.md): tabs per project,
//! each a CEF page (src/browser.rs), the address and annotation bars, the
//! popovers and panels, the history, and what a chat asks of a drawer tab.
//!
//! Kept as the web's localStorage.crundi_brd, in desktop-drawer.json in the
//! client's config folder: { open, width, side, projects: { alias: { active,
//! tabs: [{ id, url, title, device, custom, network }] } } }.
//!
//! Nothing loads by itself (the browser host starts with the first page):
//! after a relaunch the drawer starts closed, its tabs listed but not
//! loaded; opening it loads the active tab only, the others as they are
//! shown or used. Closed for the browser host's idle grace, the drawer's
//! pages are let go (as a relaunch leaves them), so the host can stop.
//!
//! Ported from the Slint client's src/desktop/drawer.rs: the logic is the
//! same; what it set on the Slint globals is state here, which the drawer's
//! view (ui.rs) reads (`ctx::dirty` asks it to draw again).

use crate::{ctx as kit, rules};
use crundi_theme::IconName;
use crundi_webview::{engine as browser, later};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::BTreeMap, time::Duration};

pub const START: &str = "https://www.google.com";
pub const MAX_TABS: usize = 12;
pub const DEFAULT_W: f32 = 560.0;
pub const MIN_W: f32 = 320.0;
/// A tab not shown and not used for this long sleeps; asleep this long, it unloads.
pub const SLEEP_MS: u64 = 5 * 60 * 1000;
pub const UNLOAD_MS: u64 = 60 * 60 * 1000;

#[derive(Clone, Debug, Default)]
pub struct Tab {
    pub id: String,
    pub url: String,
    pub title: String,
    /// An index into browser::DEVICES, or None for "custom".
    pub device: Option<usize>,
    pub custom: Option<(i32, i32)>,
    pub network: String,
    /// Its page (a panel in src/browser.rs), once made.
    pub panel: Option<usize>,
    pub diag: String,
    pub used: u64,
    pub sleeping: bool,
    pub unloaded: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Proj {
    pub active: Option<String>,
    pub tabs: Vec<Tab>,
}

#[derive(Default)]
pub struct Drawer {
    pub open: bool,
    pub width: f32,
    pub side: String,
    pub projects: BTreeMap<String, Proj>,
    pub project: String,
    pub annotating: bool,
    pub chats: Vec<(String, String)>,
    pub chat: Option<String>,
    /// The mark whose note is being typed: (panel, mark).
    pub note: Option<(usize, usize)>,
    pub pop: String,
    pub panel: String,
    pub history: Vec<rules::Visit>,
    pub history_dirty: bool,
    pub typed: String,
    /// The server's network: "local" or "remote", and how it is reached.
    pub mode: String,
    pub status: String,
    pub tool: String,
    /// The address field's suggestions (from the history).
    pub suggest: Vec<rules::Visit>,
    /// The History panel's search.
    pub hist_q: String,
    /// The Clear browsing data panel's choices.
    pub clr: Clear,
    /// Counts closings: the last one lets the pages go after the grace.
    pub closed_seq: u64,
}

/// The Clear browsing data panel (VD clear): one site or all, and what.
#[derive(Default, Clone)]
pub struct Clear { pub site_only: bool, pub site: String, pub history: bool, pub cookies: bool, pub cache: bool, pub passwords: bool }

/// One row of a popover (Device, Network, the drawer's menu, the chat to send to).
#[derive(Clone, Default, Debug)]
pub struct PopItem { pub id: String, pub label: String, pub kind: &'static str, pub on: bool, pub icon: Option<IconName> }

thread_local! {
    pub static D: RefCell<Drawer> = RefCell::new(Drawer { width: DEFAULT_W, side: "right".into(), tool: "pick".into(), mode: "remote".into(), status: "offline".into(), ..Default::default() });
}

pub fn with<R>(f: impl FnOnce(&mut Drawer) -> R) -> R { D.with(|d| f(&mut d.borrow_mut())) }

fn now() -> u64 { browser::now_ms() }

fn file(name: &str) -> std::path::PathBuf { crundi_core::store::config_dir().join(name) }

// ─── Saving ───

fn load() {
    let v: Value = std::fs::read(file("desktop-drawer.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    let h: Vec<rules::Visit> = std::fs::read(file("drawer-history.json")).ok().and_then(|b| serde_json::from_slice::<Vec<Value>>(&b).ok()).unwrap_or_default()
        .iter().filter_map(|h| Some(rules::Visit { url: h.get("url")?.as_str()?.to_string(), title: kit::s(h, "title"), at: kit::n(h, "at") as u64, visits: kit::n(h, "visits") as u64 })).collect();
    with(|d| {
        d.history = h;
        // Closed at start, whatever it was: nothing loads until it is opened.
        d.open = false;
        d.width = v.get("width").and_then(|x| x.as_f64()).map(|w| w as f32).unwrap_or(DEFAULT_W).max(MIN_W);
        d.side = if kit::s(&v, "side") == "left" { "left".into() } else { "right".into() };
        if let Some(ps) = v.get("projects").and_then(|p| p.as_object()) {
            for (alias, p) in ps {
                let tabs = p.get("tabs").and_then(|t| t.as_array()).cloned().unwrap_or_default().iter().filter_map(|t| {
                    let id = kit::s(t, "id");
                    if !rules::valid_tab_id(&id) { return None; }
                    let device = match t.get("device") { Some(Value::String(s)) if s == "custom" => None, Some(x) => Some((x.as_u64().unwrap_or(0) as usize).min(browser::DEVICES.len() - 1)), None => Some(0) };
                    let custom = t.get("custom").and_then(|c| Some((c.get("w")?.as_i64()? as i32, c.get("h")?.as_i64()? as i32)));
                    Some(Tab { id, url: kit::s(t, "url"), title: kit::s(t, "title"), device: if device.is_none() && custom.is_none() { Some(0) } else { device }, custom, network: rules::norm_network(&kit::s(t, "network")).unwrap_or_else(|| "client".into()), ..Default::default() })
                }).collect::<Vec<_>>();
                let active = Some(kit::s(p, "active")).filter(|a| tabs.iter().any(|t| &t.id == a));
                d.projects.insert(alias.to_lowercase(), Proj { active, tabs });
            }
        }
    });
}

pub fn persist() {
    let v = with(|d| json!({
        "open": d.open, "width": d.width, "side": d.side,
        "projects": d.projects.iter().map(|(k, p)| (k.clone(), json!({
            "active": p.active,
            "tabs": p.tabs.iter().map(|t| json!({ "id": t.id, "url": t.url, "title": t.title,
                "device": match t.device { Some(i) => json!(i), None => json!("custom") },
                "custom": t.custom.map(|(w, h)| json!({ "w": w, "h": h })), "network": t.network })).collect::<Vec<_>>(),
        }))).collect::<serde_json::Map<_, _>>(),
    }));
    write_atomic("desktop-drawer.json", &v);
}

fn write_atomic(name: &str, v: &Value) {
    let p = file(name);
    let tmp = p.with_extension("tmp");
    if let Some(dir) = p.parent() { let _ = std::fs::create_dir_all(dir); }
    if std::fs::write(&tmp, serde_json::to_vec(v).unwrap_or_default()).is_ok() { let _ = std::fs::rename(&tmp, &p); }
}

/// The history goes to disk a second after the last visit (BD saveHistory).
fn save_history_soon() {
    if with(|d| std::mem::replace(&mut d.history_dirty, true)) { return; }
    later(Duration::from_secs(1), || {
        let v = with(|d| { d.history_dirty = false; Value::Array(d.history.iter().map(|h| json!({ "url": h.url, "title": h.title, "at": h.at, "visits": h.visits })).collect()) });
        write_atomic("drawer-history.json", &v);
    });
}

// ─── Tabs ───

fn key(p: &str) -> String { p.to_lowercase() }

pub fn project_key() -> String { with(|d| key(&d.project)) }

/// Find a tab by id in any project: (project, tab).
pub fn find(id: &str) -> Option<(String, Tab)> {
    with(|d| d.projects.iter().find_map(|(p, ps)| ps.tabs.iter().find(|t| t.id == id).map(|t| (p.clone(), t.clone()))))
}

fn tab_mut<R>(id: &str, f: impl FnOnce(&mut Tab) -> R) -> Option<R> {
    with(|d| d.projects.values_mut().find_map(|ps| ps.tabs.iter_mut().find(|t| t.id == id)).map(f))
}

pub fn active_tab() -> Option<Tab> {
    with(|d| {
        let ps = d.projects.get(&key(&d.project))?;
        ps.tabs.iter().find(|t| Some(&t.id) == ps.active.as_ref()).or(ps.tabs.first()).cloned()
    })
}

/// The tab a panel shows.
pub fn tab_of_panel(i: usize) -> Option<Tab> {
    with(|d| d.projects.values().find_map(|ps| ps.tabs.iter().find(|t| t.panel == Some(i)).cloned()))
}

/// A new tab in [project] (VD newTab): at most 12; made active when asked
/// (or none is). Its page is made now when [load].
pub fn new_tab(project: &str, url: &str, network: &str, activate: bool, load: bool) -> Result<String, String> {
    if project.is_empty() { return Err("Select a project first".into()); }
    let p = key(project);
    if with(|d| d.projects.get(&p).map(|ps| ps.tabs.len()).unwrap_or(0)) >= MAX_TABS { return Err(format!("At most {MAX_TABS} tabs per project")); }
    let id = rules::tab_id(now(), rand());
    with(|d| {
        let ps = d.projects.entry(p).or_default();
        ps.tabs.push(Tab { id: id.clone(), url: url.to_string(), network: network.to_string(), device: Some(0), used: now(), ..Default::default() });
        if activate || ps.active.is_none() { ps.active = Some(id.clone()); }
    });
    if load { ensure(&id); }
    persist();
    render();
    Ok(id)
}

fn rand() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(now());
    h.finish()
}

/// The tab's page, made if it has none (a restored tab is made lazily, VD
/// ensure): returns its panel.
pub fn ensure(id: &str) -> Option<usize> {
    let t = find(id)?.1;
    // A page the browser host lost (it crashed) is made again on use.
    if let Some(i) = t.panel.filter(|i| browser::with(|st| st.panels.get(*i).is_some_and(|p| p.browser.is_none() && p.crashed))) {
        browser::close_panel(i);
        tab_mut(id, |x| x.panel = None);
    } else if let Some(i) = t.panel { wake(id); return Some(i); }
    let url = if t.url.is_empty() { "about:blank".to_string() } else { t.url.clone() };
    let i = browser::open_panel(&url, &t.network);
    browser::with(|st| { let p = &mut st.panels[i]; p.tab = Some(id.to_string()); p.device = t.device.unwrap_or(0); p.custom = if t.device.is_none() { t.custom } else { None }; p.title = t.title.clone(); });
    tab_mut(id, |x| { x.panel = Some(i); x.diag.clear(); x.used = now(); x.sleeping = false; x.unloaded = false; });
    crate::passwords::attach(i);
    if t.device != Some(0) { let i2 = i; later(Duration::from_millis(300), move || browser::apply_device(i2)); }
    Some(i)
}

/// Close a tab (VD closeTab): its page goes; the left neighbour is shown.
pub fn close_tab(id: &str) {
    let Some((p, t)) = find(id) else { return };
    if let Some(i) = t.panel { browser::close_panel(i); }
    with(|d| {
        let ps = d.projects.get_mut(&p).unwrap();
        let at = ps.tabs.iter().position(|x| x.id == id).unwrap_or(0);
        ps.tabs.remove(at);
        if ps.active.as_deref() == Some(id) {
            ps.active = ps.tabs.get(at.saturating_sub(1)).or(ps.tabs.first()).map(|x| x.id.clone());
        }
        if d.note.is_some_and(|(n, _)| Some(n) == t.panel) { d.note = None; }
    });
    persist();
    show_active();
}

pub fn activate(id: &str) {
    let Some((p, _)) = find(id) else { return };
    with(|d| { if let Some(ps) = d.projects.get_mut(&p) { ps.active = Some(id.to_string()); } d.note = None; });
    persist();
    show_active();
}

/// Show the current project's active tab (VD showActive): its page made if
/// needed and put on the stage.
pub fn show_active() {
    let open = with(|d| d.open);
    let t = active_tab();
    let _panel = match (&t, open) { (Some(t), true) => ensure(&t.id), (Some(t), false) => t.panel, _ => None };
    if t.is_none() { with(|d| d.annotating = false); }
    if let Some(t) = &t { tab_mut(&t.id, |x| x.used = now()); }
    render();
}

/// The tab bar's Browser (and the server's show): open, or close.
pub fn toggle() {
    if kit::project().is_empty() { return kit::toast_error("Select a project first"); }
    if with(|d| d.open) { close() } else { open() }
}

/// The shell shows or hides the drawer beside the project area.
fn shell_drawer(open: bool) { kit::on_app(move |_, cx| crundi_shell::set_drawer(cx, open)); }

pub fn open() {
    with(|d| d.open = true);
    persist();
    shell_drawer(true);
    let p = kit::project();
    if !p.is_empty() && with(|d| d.projects.get(&key(&p)).map(|ps| ps.tabs.is_empty()).unwrap_or(true)) {
        let _ = new_tab(&p, START, "client", true, true);
    }
    show_active();
}

pub fn close() {
    let seq = with(|d| { d.open = false; d.annotating = false; d.pop.clear(); d.note = None; d.closed_seq += 1; d.closed_seq });
    persist();
    shell_drawer(false);
    render();
    // Still closed after the grace: the pages go (listed, loaded again when shown).
    later(crundi_webview::idle_grace(), move || if with(|d| !d.open && d.closed_seq == seq) { unload_all(); });
}

/// Every tab's page goes; the tabs stay, as after a relaunch.
pub fn unload_all() {
    let tabs: Vec<(String, usize)> = with(|d| d.projects.values().flat_map(|ps| ps.tabs.iter().filter_map(|t| t.panel.map(|i| (t.id.clone(), i)))).collect::<Vec<_>>());
    for (id, i) in tabs {
        browser::close_panel(i);
        tab_mut(&id, |x| { x.panel = None; x.sleeping = false; });
    }
    render();
}

/// Reload a tab's page that the browser host lost: made again, as a tab
/// is (its device, its passwords script). False: not a drawer tab's page.
pub fn reload_panel(i: usize) -> bool {
    let Some(t) = tab_of_panel(i) else { return false };
    browser::close_panel(i);
    tab_mut(&t.id, |x| x.panel = None);
    ensure(&t.id);
    render();
    true
}

/// Close every tab of this project, then hide (VD closeAll).
pub fn close_all() {
    let ids: Vec<String> = with(|d| d.projects.get(&key(&d.project)).map(|ps| ps.tabs.iter().map(|t| t.id.clone()).collect()).unwrap_or_default());
    for id in ids { close_tab(&id); }
    close();
}

/// The project changed: its tabs, its active tab; annotating ends.
pub fn set_project(alias: &str) {
    with(|d| { d.project = alias.to_string(); d.annotating = false; d.note = None; d.pop.clear(); d.panel.clear(); });
    show_active();
}

// ─── The address bar ───

pub fn go(typed: &str) {
    let url = rules::url_of(typed);
    with(|d| d.typed.clear());
    if url.is_empty() { return; }
    let Some(t) = active_tab() else { return };
    if !rules::ok_url(&url) { return kit::toast_error("Only http and https addresses open here."); }
    let Some(i) = ensure(&t.id) else { return };
    tab_mut(&t.id, |x| x.url = url.clone());
    browser::navigate(i, &url, Box::new(|| {}));
    render();
}

pub fn url_typed(text: &str) {
    with(|d| d.typed = text.to_string());
    // 120 ms after typing stops, the history's best eight (VD urlEl input).
    let t = text.to_string();
    later(Duration::from_millis(120), move || {
        if with(|d| d.typed != t) { return; }
        let rows = if t.trim().is_empty() { vec![] } else { with(|d| rules::query(&d.history, &t, 8)) };
        with(|d| d.suggest = rows);
        render();
    });
}

/// A page's address changed: the history keeps http(s) visits.
pub fn visited(i: usize, url: &str) {
    let Some(t) = tab_of_panel(i) else { return };
    let title = browser::with(|st| st.panels[i].title.clone());
    tab_mut(&t.id, |x| { x.url = url.to_string(); x.diag.clear(); });
    if with(|d| rules::record(&mut d.history, url, &title, now())) { save_history_soon(); }
    persist();
}

/// A panel's state changed (title, url, loading, marks).
pub fn panel_changed(i: usize) {
    let Some(t) = tab_of_panel(i) else { return };
    let (title, url) = browser::with(|st| (st.panels[i].title.clone(), st.panels[i].url.clone()));
    let changed = tab_mut(&t.id, |x| {
        let c = x.title != title || (!url.is_empty() && x.url != url);
        if !title.is_empty() { x.title = title.clone(); }
        if !url.is_empty() { x.url = url.clone(); }
        c
    }).unwrap_or(false);
    if changed {
        if !title.is_empty() && !url.is_empty() { with(|d| if let Some(h) = d.history.iter_mut().find(|h| h.url == url) { h.title = title.clone(); }); }
        persist();
    }
    // Only what the drawer shows needs drawing again.
    if changed || active_tab().is_some_and(|a| a.id == t.id) { render_soon(); }
}

/// A link that opens a window: a new tab in the same project and network,
/// made active, the drawer not popped open (BD setWindowOpenHandler).
pub fn popup(i: usize, url: &str) -> bool {
    let Some((p, t)) = tab_of_panel(i).and_then(|t| find(&t.id)) else { return false };
    if !rules::ok_url(url) { return true; }
    match new_tab(&p, url, &t.network, true, true) {
        Ok(_) => { show_active(); }
        Err(e) => kit::toast_error(&e),
    }
    true
}

// ─── Devices and networks ───

pub fn set_device(id: &str, d: usize) {
    let Some(t) = find(id).map(|x| x.1) else { return };
    tab_mut(id, |x| { x.device = Some(d.min(browser::DEVICES.len() - 1)); x.custom = None; });
    if let Some(i) = t.panel { browser::set_device(i, d); }
    persist();
    render();
}

/// A chat's browser_resize on a drawer tab: a custom size.
pub fn set_custom(id: &str, w: i32, h: i32) {
    let (w, h) = (w.clamp(200, 3840), h.clamp(200, 3840));
    let Some(t) = find(id).map(|x| x.1) else { return };
    tab_mut(id, |x| { x.device = None; x.custom = Some((w, h)); });
    if let Some(i) = t.panel { browser::resize(i, w, h); }
    persist();
    render();
}

/// Another network: the page is made again in that network's profile at its
/// address; the marks stay (VD set-network).
pub fn set_network(id: &str, network: &str) {
    let Some(t) = find(id).map(|x| x.1) else { return };
    if t.network == network { return; }
    let marks = t.panel.map(|i| browser::with(|st| st.panels[i].ann.clone())).unwrap_or_default();
    if let Some(i) = t.panel { browser::close_panel(i); }
    tab_mut(id, |x| { x.network = network.to_string(); x.panel = None; });
    let shown = with(|d| d.open) && active_tab().is_some_and(|a| a.id == id);
    if shown || t.panel.is_some() {
        if let Some(i) = ensure(id) { browser::with(|st| st.panels[i].ann = marks); browser::sync(i); }
    }
    persist();
    show_active();
}

pub fn label(network: &str) -> String {
    let (mode, status) = with(|d| (d.mode.clone(), d.status.clone()));
    let hp = network.strip_prefix("hp:").and_then(crate::hp_name);
    rules::network_label(network, &mode, &status, hp.as_deref())
}

fn server_bad() -> bool { with(|d| d.mode == "remote" && d.status != "tunnel") }

// ─── Annotating ───

pub fn annotate(on: bool) {
    let on = on && active_tab().is_some();
    with(|d| { d.annotating = on; d.note = None; d.pop.clear(); });
    if !on { if let Some(i) = active_tab().and_then(|t| t.panel) { browser::with(|st| st.panels[i].hover = None); browser::sync(i); } }
    if on { fill_chats(); }
    render();
}

pub fn set_tool(t: &str) {
    browser::TOOL.with(|x| *x.borrow_mut() = t.to_string());
    with(|d| { d.tool = t.to_string(); d.note = None; });
    render();
}

/// The chats a mark can go to (WH brdChats): this project's open chats, Cru
/// first for the owner; the last pick, else the focused chat, else the first.
pub fn fill_chats() {
    let c = kit::get();
    let panes: Vec<String> = c.panes.iter().filter_map(|k| k.strip_prefix("live:").map(str::to_string)).collect();
    let p = c.project.to_lowercase();
    let mut list: Vec<(String, String)> = c.terminals.iter()
        .filter(|t| kit::s(t, "kind") == "ui" && kit::s(t, "status") != "exited" && kit::s(t, "project").to_lowercase() == p && panes.contains(&kit::s(t, "id")))
        .map(|t| (kit::s(t, "id"), Some(kit::s(t, "title")).filter(|x| !x.is_empty()).unwrap_or_else(|| "Chat".into()))).collect();
    if crate::cru_available() { list.insert(0, ("cru".into(), "Cru".into())); }
    let focused = c.focused.clone();
    with(|d| {
        let keep = d.chat.clone().filter(|c| list.iter().any(|(id, _)| id == c));
        d.chat = keep.or_else(|| list.iter().find(|(id, _)| *id == focused).map(|x| x.0.clone())).or_else(|| list.first().map(|x| x.0.clone()));
        d.chats = list;
    });
}

/// A mark was made: its note field opens by it (VD askNote).
pub fn ask_note(i: usize, n: usize) {
    with(|d| d.note = Some((i, n)));
    render();
}

pub fn note(text: &str, keep: bool) {
    let Some((i, n)) = with(|d| d.note.take()) else { return };
    if keep && !text.trim().is_empty() { browser::set_note(i, n, text); }
    render();
}

/// Send the marks to the chosen chat (VD sendToChat): with the screenshot,
/// or the selected elements' selectors as text.
pub fn send(with_shot: bool) {
    let Some(t) = active_tab() else { return };
    let Some(i) = t.panel else { return };
    let Some(chat) = with(|d| d.chat.clone()) else { return kit::toast_error("Open a chat in this project first") };
    let mut list: Vec<Value> = browser::with(|st| st.panels[i].ann.iter().filter(|a| a.by != "claude").cloned().collect::<Vec<_>>())
        .iter().enumerate().map(|(n, a)| browser::ann_json(n + 1, a)).collect();
    for (n, a) in list.iter_mut().enumerate() { a["n"] = json!(n + 1); }
    let drawn = list.iter().any(|a| a.get("picked").is_none());
    if !with_shot && (drawn || list.is_empty()) { return render(); }
    let (vw, vh) = browser::with(|st| { let (w, h, ..) = st.panels[i].view(); (w, h) });
    let url = browser::with(|st| st.panels[i].url.clone());
    let url = if url.is_empty() { t.url.clone() } else { url };
    let text = rules::marks_text(&url, &t.title, vw, vh, t.network == "server", &t.id, with_shot, &list);
    let deliver = move |png: Option<Vec<u8>>| crate::marks::send_to_chat(&chat, &text, png);
    if with_shot {
        crundi_webview::shot::composite(i, true, Box::new(move |r| match r {
            Ok(png) => deliver(Some(png)),
            Err(e) => kit::toast_error(&format!("Could not attach: {e}")),
        }));
    } else {
        deliver(None);
    }
}

// ─── Popovers ───

pub fn pop_open(which: &str) {
    let same = with(|d| d.pop == which);
    with(|d| d.pop = if same { String::new() } else { which.to_string() });
    if which == "chat" && !same { fill_chats(); }
    if which == "net" && !same { crate::hp_refresh(); }
    if which == "passwords" { with(|d| d.pop.clear()); return crate::passwords::site_menu(); }
    render();
}

pub fn pop_pick(id: &str) {
    let which = with(|d| std::mem::take(&mut d.pop));
    if id.is_empty() { return render(); }
    let t = active_tab();
    match which.as_str() {
        "dev" => if let Some(t) = t { if let Ok(n) = id.parse::<usize>() { set_device(&t.id, n); } },
        "net" => match id {
            "hp-settings" => open_panel("homepool"),
            n => if let Some(t) = t { set_network(&t.id, n); },
        },
        "chat" => { with(|d| d.chat = Some(id.to_string())); }
        "menu" => match id {
            "history" => open_panel("history"),
            "passwords" => open_panel("passwords"),
            "clear" => open_panel("clear"),
            "side" => {
                with(|d| d.side = if d.side == "left" { "right".into() } else { "left".into() });
                persist();
            }
            _ => {}
        },
        _ => {}
    }
    render();
}

pub fn pop_items(which: &str) -> (String, Vec<PopItem>) {
    let item = |id: &str, label: &str, on: bool| PopItem { id: id.into(), label: label.into(), kind: "item", on, icon: None };
    let t = active_tab();
    match which {
        "dev" => {
            let mut v: Vec<_> = browser::DEVICES.iter().enumerate().map(|(n, d)| item(&n.to_string(), &d.label(), t.as_ref().is_some_and(|t| t.device == Some(n)))).collect();
            if let Some((w, h)) = t.as_ref().filter(|t| t.device.is_none()).and_then(|t| t.custom) { v.push(item("custom", &format!("Custom  {w}x{h}"), true)); }
            ("Device size".into(), v)
        }
        "net" => {
            let cur = t.as_ref().map(|t| t.network.clone()).unwrap_or_default();
            let mut v = vec![];
            if cur.starts_with("hp:") { v.push(PopItem { label: format!("{}: {}", label(&cur), crate::exit_text(&cur)), kind: "note", ..Default::default() }); }
            v.push(item("client", &label("client"), cur == "client"));
            v.push(item("server", &label("server"), cur == "server"));
            let nets = crate::hp_offered();
            if !nets.is_empty() {
                v.push(PopItem { label: "Homepool".into(), kind: "group", ..Default::default() });
                for (id, name, exit) in nets { v.push(item(&format!("hp:{id}"), &format!("{name}{}", if exit { "" } else { " (no exit up)" }), cur == format!("hp:{id}"))); }
            }
            if crate::hp_available() { v.push(item("hp-settings", if crate::hp_has_key() { "Homepool settings…" } else { "Homepool: set up…" }, false)); }
            ("This tab’s network".into(), v)
        }
        "menu" => {
            let left = with(|d| d.side == "left");
            let v = vec![
                PopItem { icon: Some(IconName::Clock), ..item("history", "History", false) },
                PopItem { icon: Some(IconName::Lock), ..item("passwords", "Passwords", false) },
                PopItem { icon: Some(IconName::Trash), ..item("clear", "Clear browsing data…", false) },
                PopItem { icon: Some(if left { IconName::ChevronRight } else { IconName::ChevronLeft }), ..item("side", if left { "Open on the right" } else { "Open on the left" }, false) },
            ];
            (String::new(), v)
        }
        "chat" => {
            let (chats, cur) = with(|d| (d.chats.clone(), d.chat.clone()));
            let v = if chats.is_empty() { vec![PopItem { label: "No chat open in this project".into(), kind: "note", ..Default::default() }] }
                else { chats.iter().map(|(id, title)| item(id, title, cur.as_deref() == Some(id))).collect() };
            ("Chat to send to".into(), v)
        }
        _ => (String::new(), vec![]),
    }
}

// ─── Panels: History, Clear browsing data ───

pub fn open_panel(which: &str) {
    with(|d| { d.panel = which.to_string(); d.pop.clear(); });
    match which {
        "history" => with(|d| d.hist_q.clear()),
        "clear" => {
            let site = active_tab().map(|t| rules::norm_site(&rules::host_of(&t.url)).unwrap_or_default()).unwrap_or_default();
            with(|d| d.clr = Clear { site_only: false, site, history: true, cookies: false, cache: true, passwords: false });
        }
        "passwords" => crate::passwords::show_panel(),
        "homepool" => {}
        _ => {}
    }
    render();
}

pub fn panel_close() {
    with(|d| d.panel.clear());
    crate::passwords::panel_closed();
    render();
}

pub fn history_query(q: &str) {
    with(|d| d.hist_q = q.to_string());
    render();
}

/// The History panel's rows: (url, title, how long ago), and its empty line.
pub fn history_rows() -> (Vec<(String, String, String)>, &'static str) {
    let t = now();
    with(|d| {
        let rows = rules::query(&d.history, &d.hist_q, 200).iter().map(|h| (h.url.clone(), h.title.clone(), rules::ago(t, h.at))).collect();
        (rows, if d.hist_q.trim().is_empty() { "No history yet." } else { "Nothing in the history matches." })
    })
}

pub fn history_open(url: &str) {
    with(|d| d.panel.clear());
    match active_tab() {
        Some(_) => go(url),
        None => { let _ = new_tab(&kit::project(), url, "client", true, true); show_active(); }
    }
    render();
}

/// The Clear button (VD doClear).
pub fn clear_run() {
    let c = with(|d| d.clr.clone());
    let (hist, cookies, cache, pw) = (c.history, c.cookies, c.cache, c.passwords);
    if !hist && !cookies && !cache && !pw { return kit::toast_error("Choose something to clear"); }
    let site = if c.site_only {
        match rules::norm_site(&c.site) { Some(s) => Some(s), None => return kit::toast_error("Type a site, such as example.com") }
    } else { None };
    let run = move || {
        let mut cleared = vec![];
        if hist {
            with(|d| match &site {
                Some(s) => d.history.retain(|h| { let host = rules::host_of(&h.url); let host = host.split(':').next().unwrap_or(""); !(host == s || host.ends_with(&format!(".{s}"))) }),
                None => d.history.clear(),
            });
            save_history_soon();
            cleared.push("history");
        }
        if cookies || cache { crundi_webview::shot::clear_data(site.as_deref(), cookies, cache); }
        if cookies { cleared.push("cookies and site data"); }
        if cache { cleared.push("cached files"); }
        let what = if cleared.is_empty() { "nothing".to_string() } else { cleared.join(", ") };
        if !(pw && cleared.is_empty()) { kit::toast_ok(&format!("Cleared{}: {what}", site.as_ref().map(|s| format!(" for {s}")).unwrap_or_default())); }
        with(|d| d.panel.clear());
        render();
    };
    if pw {
        let run = std::cell::RefCell::new(Some(run));
        kit::confirm("Delete every saved password on this computer?", "Delete", true, move |ok| {
            if ok { crate::passwords::clear_all(); kit::toast_ok("Saved passwords deleted"); }
            if let Some(r) = run.borrow_mut().take() { r(); }
        });
    } else {
        run();
    }
}

// ─── Sleep ───

/// Every minute: a tab not shown nor used for 5 minutes sleeps (frozen), an
/// hour asleep unloads (its page goes; it loads again when shown or used).
pub fn sleep_tick() {
    let t = now();
    let shown = with(|d| d.open).then(active_tab).flatten().map(|a| a.id);
    let tabs: Vec<Tab> = with(|d| d.projects.values().flat_map(|ps| ps.tabs.clone()).collect());
    for x in tabs {
        let Some(i) = x.panel else { continue };
        if Some(&x.id) == shown.as_ref() { continue; }
        let idle = t.saturating_sub(x.used);
        if !x.sleeping && idle > SLEEP_MS {
            browser::cdp(i, "Page.setWebLifecycleState", json!({ "state": "frozen" }), Box::new(|_, _| {}));
            tab_mut(&x.id, |y| y.sleeping = true);
        } else if x.sleeping && idle > SLEEP_MS + UNLOAD_MS {
            browser::close_panel(i);
            tab_mut(&x.id, |y| { y.panel = None; y.unloaded = true; });
        }
    }
    render();
}

/// A tab is used (shown, or by a chat): awake.
pub fn wake(id: &str) {
    let t = find(id).map(|x| x.1);
    tab_mut(id, |x| x.used = now());
    if let Some(t) = t.filter(|t| t.sleeping) {
        if let Some(i) = t.panel { browser::cdp(i, "Page.setWebLifecycleState", json!({ "state": "active" }), Box::new(|_, _| {})); }
        tab_mut(id, |x| { x.sleeping = false; x.unloaded = false; });
    }
}

// ─── Drawing ───

/// The drawer's view draws again (at the next pump).
pub fn render() { kit::dirty(); }
fn render_soon() { kit::dirty(); }

/// What the drawer shows of a tab: (id, title, tooltip, badge, bad, active, asleep).
pub struct TabView { pub id: String, pub title: String, pub tip: String, pub badge: &'static str, pub bad: bool, pub active: bool, pub sleeping: bool }

pub fn tab_views() -> Vec<TabView> {
    let proj = with(|d| d.projects.get(&key(&d.project)).cloned().unwrap_or_default());
    let act = active_tab();
    let bad = server_bad();
    proj.tabs.iter().map(|t| {
        let hp = t.network.starts_with("hp:");
        let hp_bad = hp && !crate::hp_exit_up(&t.network);
        let title = if !t.title.is_empty() { t.title.clone() } else if !t.url.is_empty() { t.url.clone() } else { "New tab".into() };
        let mut tip = format!("{}\n{}\nNetwork: {}", if t.title.is_empty() { &t.url } else { &t.title }, t.url, label(&t.network));
        if t.unloaded { tip.push_str("\nUnloaded after an hour asleep: loads again when shown or used"); } else if t.sleeping { tip.push_str("\nAsleep: wakes, as it was, when shown or used"); }
        TabView {
            id: t.id.clone(), title, tip,
            badge: if t.network == "server" { "SRV" } else if hp { "HP" } else { "" },
            bad: (t.network == "server" && bad) || hp_bad,
            active: act.as_ref().is_some_and(|a| a.id == t.id),
            sleeping: t.sleeping || t.unloaded,
        }
    }).collect()
}

/// The address bar's buttons: the Network button's tip and tone, the Device button's tip and whether it is on.
pub fn bar_state() -> (String, &'static str, String, bool) {
    let bad = server_bad();
    match active_tab() {
        Some(t) => {
            let hp = t.network.starts_with("hp:");
            let tip = format!("Network: {}{}", label(&t.network), if hp { format!("\n{}", crate::exit_text(&t.network)) } else { String::new() });
            let tone = if (hp && !crate::hp_exit_up(&t.network)) || (t.network == "server" && bad) { "bad" } else if hp { "hp" } else if t.network == "server" { "server" } else { "" };
            let dev = match (t.device, t.custom) {
                (None, Some((cw, ch))) => format!("Device size: Custom {cw}x{ch}"),
                (None, None) => "Device size: Custom".into(),
                (Some(n), _) => format!("Device size: {}", browser::DEVICES[n.min(browser::DEVICES.len() - 1)].name),
            };
            (tip, tone, dev, t.device != Some(0))
        }
        None => ("Network".into(), "", "Device size: Responsive".into(), false),
    }
}

/// The annotation bar: the count line, and the two send buttons (ok, tip).
pub fn ann_bar() -> (String, (bool, String), (bool, String)) {
    let act = active_tab();
    let chat = with(|d| d.chat.clone());
    let (mine, theirs, drawn, picked) = act.as_ref().and_then(|t| t.panel).map(|i| browser::with(|st| {
        let a = &st.panels[i].ann;
        let mine: Vec<_> = a.iter().filter(|x| x.by != "claude").collect();
        (mine.len(), a.len() - mine.len(), mine.iter().any(|x| x.kind != "element" && x.selector.is_empty()), mine.iter().any(|x| x.kind == "element" || !x.selector.is_empty()))
    })).unwrap_or((0, 0, false, false));
    let no_chat = chat.is_none();
    let css = (!no_chat && !drawn && picked, if no_chat { "Open a chat in this project first" }
        else if drawn { "CSS selectors: not available while something is drawn on the page (a drawing has no selector). Send the full screenshot, or clear the drawing." }
        else if !picked { "CSS selectors: select an element first" }
        else { "Send to the chat: the selected elements, with their notes and CSS selectors (no picture)" }.to_string());
    let shot = (!no_chat, if no_chat { "Open a chat in this project first".to_string() } else { format!("Send to the chat: the full screenshot with your marks drawn in{}", if picked { ", and the selected elements' selectors" } else { "" }) });
    (rules::count_text(mine, theirs), css, shot)
}

/// Where the note field sits (stage px), while a mark's note is being typed.
pub fn note_at() -> Option<(f32, f32)> {
    with(|d| d.note).and_then(|(i, n)| browser::with(|st| {
        let p = st.panels.get(i)?;
        let a = p.ann.get(n)?;
        let boxy = a.kind == "box" || a.kind == "element";
        let (x, y) = if boxy { (a.x + a.w, a.y + a.h) } else { (a.x, a.y) };
        let k = p.fit_scale();
        Some((x * k.0 + k.1, y * k.0 + k.2))
    }))
}

// ─── Start ───

pub fn init() {
    load();
    fn tick() { sleep_tick(); later(Duration::from_secs(60), tick); }
    later(Duration::from_secs(60), tick);
    render();
}

/// The driver's view of the drawer.
pub fn props() -> Value {
    let act = active_tab();
    with(|d| json!({
        "open": d.open, "width": d.width, "side": d.side, "project": d.project,
        "annotating": d.annotating, "tool": d.tool, "pop": d.pop, "panel": d.panel,
        "chats": d.chats.iter().map(|(id, t)| json!({ "id": id, "title": t })).collect::<Vec<_>>(), "chat": d.chat,
        "note": d.note.map(|(i, n)| json!([i, n])),
        "active": act.as_ref().map(|t| t.id.clone()),
        "history": d.history.len(),
        "tabs": d.projects.get(&key(&d.project)).map(|ps| ps.tabs.iter().map(|t| json!({
            "id": t.id, "url": t.url, "title": t.title, "network": t.network, "panel": t.panel,
            "device": t.device, "custom": t.custom.map(|(w, h)| json!([w, h])), "sleeping": t.sleeping, "unloaded": t.unloaded,
            "crashed": t.panel.is_some_and(browser::crashed),
        })).collect::<Vec<_>>()).unwrap_or_default(),
        "projects": d.projects.iter().map(|(k, p)| (k.clone(), json!(p.tabs.len()))).collect::<serde_json::Map<_, _>>(),
    }))
}


