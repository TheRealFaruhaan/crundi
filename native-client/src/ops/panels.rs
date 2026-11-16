//! Ops area: Panels: the widgets tab, the widget panel and the inline widget card (crundi-widgets.js).
//! UI: ui/ops/panels.slint (global OpsPanels).
//!
//! A panel ("widget") is HTML Claude wrote. The web puts it in a sandboxed
//! iframe and talks to it with postMessage (app/vendor/crundi-widgets.js);
//! here every copy on screen is an off-screen CEF view of src/frames.rs, by
//! frame key "wg|<frame>|<project>|<id>" (frame: tab, cell, inline:<chat>,
//! dock:<chat>, chip, pop). Its box reports itself (OpsPanels.seen); the
//! first report loads the document (/api/widgets/doc) and its data
//! (/api/widgets/data), and the page is handed to frames::fetched.
//!
//! The bridge. frames.rs gives a view no JS binding and no way to run script
//! in it (shared/requests.md asks for frames::eval and a console hook), so,
//! for now:
//!  - in: the page is built with its first message (`init`: data, store,
//!    context) inside it, answered from the page itself when the runtime says
//!    `ready`. A later change (new data, a store saved elsewhere, a new
//!    version) reloads the page with the new values (the web posts them).
//!  - out: a small script ahead of the runtime takes the runtime's messages
//!    to its host (ready, height, fault, call) and writes the latest few into
//!    the page's title; the title reaches Rust (frames::list) and is read on
//!    every report. A call's answer cannot go back in (its promise times out
//!    in the page); what it does on the server it does.
//! Everything else is the web's: the list, placement, the tab's bar and
//! panes, grants, the chooser, docks, chips and their popover, events.

use crate::{api::ApiError, areas::AreaEvent, kit, MainWindow};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::{Duration, Instant}};

// ─── Pure rules (unit-tested) ───

/// The web's SLOT_LABEL.
pub const SLOT_LABEL: [(&str, &str); 5] = [("dock", "Docked to chat"), ("cell", "Workbench pane"), ("tab", "Panels tab only"), ("inline", "Inline in chat"), ("chip", "Top bar chip")];
/// Where a person may move a panel to.
pub const MOVABLE: [&str; 4] = ["dock", "cell", "tab", "chip"];

pub fn slot_label(slot: &str) -> String {
    SLOT_LABEL.iter().find(|(k, _)| *k == slot).map(|(_, l)| l.to_string()).unwrap_or_else(|| slot.to_string())
}

/// The server's key for a project (widget events carry it): lower case,
/// anything but [a-z0-9._-] as "_", "_global" for none.
pub fn pkey(alias: &str) -> String {
    let k: String = alias.to_lowercase().chars().map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c) { c } else { '_' }).collect();
    if k.is_empty() { "_global".into() } else { k }
}

/// Where a panel shows right now, given the chats on screen: "dock",
/// "cell", "chip" or "none" (the web's placeOf).
pub fn place_of(w: &Value, live_chats: &[String]) -> &'static str {
    if kit::s(w, "state") != "open" { return "none"; }
    let chat = kit::s(w, "chat");
    match kit::s(w, "slot").as_str() {
        "dock" if chat.is_empty() => "cell",
        "dock" => if live_chats.contains(&chat) { "dock" } else { "none" },
        "cell" => "cell",
        "chip" => "chip",
        _ => "none",
    }
}

/// The slot <select>: labels, values and the current one. Dock only for a
/// panel with a chat; a slot that is not movable (inline) is listed first.
pub fn slot_options(w: &Value) -> (Vec<String>, Vec<String>, i32) {
    let slot = kit::s(w, "slot");
    let chat = kit::s(w, "chat");
    let mut values: Vec<String> = MOVABLE.iter().filter(|s| !(**s == "dock" && chat.is_empty())).map(|s| s.to_string()).collect();
    if !MOVABLE.contains(&slot.as_str()) { values.insert(0, slot.clone()); }
    let labels = values.iter().map(|v| slot_label(v)).collect();
    let i = values.iter().position(|v| *v == slot).map(|i| i as i32).unwrap_or(-1);
    (labels, values, i)
}

/// A tab row's line: "closed", or its slot's first word ("docked",
/// "workbench", "panels", "inline", "top").
pub fn item_sub(w: &Value) -> String {
    if kit::s(w, "state") != "open" { return "closed".into(); }
    slot_label(&kit::s(w, "slot")).split(' ').next().unwrap_or("").to_lowercase()
}

/// A tab row's badge and its tone: waiting for approval, or faulted.
pub fn item_badge(w: &Value) -> (&'static str, &'static str) {
    if w.get("needsGrant").is_some_and(kit::truthy) { ("approve", "warn") }
    else if kit::n(w, "faults") > 0.0 { ("error", "err") }
    else { ("", "") }
}

/// The tab's order: open first, then the newest.
pub fn tab_order(list: &[Value]) -> Vec<Value> {
    let mut v = list.to_vec();
    v.sort_by(|a, b| {
        let oa = (kit::s(a, "state") != "open") as u8;
        let ob = (kit::s(b, "state") != "open") as u8;
        oa.cmp(&ob).then(kit::n(b, "updatedAt").total_cmp(&kit::n(a, "updatedAt")))
    });
    v
}

/// The chooser's line for a panel.
pub fn pick_status(w: &Value, place: &str) -> &'static str {
    if kit::s(w, "state") != "open" { "closed" }
    else { match place { "cell" => "already open", "dock" => "docked to a chat", "chip" => "top bar", _ => "Panels tab" } }
}

/// The tab bar's scope control: "local" (Make global), "global" (Global:
/// back to this project), "other" (a badge: from another project).
pub fn scope_of(w: &Value, project: &str) -> &'static str {
    if !w.get("global").is_some_and(kit::truthy) { return "local"; }
    let home = kit::s(w, "home");
    if home.is_empty() || home == pkey(project) { "global" } else { "other" }
}

/// A dotted path into a value (a chip's value).
pub fn path_get<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() { return Some(v); }
    path.split('.').try_fold(v, |cur, p| cur.get(p))
}

/// A chip's value as text: at most 60 characters, objects as JSON.
pub fn chip_text(v: Option<&Value>) -> String {
    let t = match v { None | Some(Value::Null) => String::new(), Some(Value::String(s)) => s.clone(), Some(o) => o.to_string() };
    t.chars().take(60).collect()
}

/// A frame key's parts: (frame, project, id).
pub fn parse_key(key: &str) -> Option<(String, String, String)> {
    let mut it = key.splitn(4, '|');
    if it.next()? != "wg" { return None; }
    let (f, p, i) = (it.next()?, it.next()?, it.next()?);
    if f.is_empty() || i.is_empty() { return None; }
    Some((f.to_string(), p.to_string(), i.to_string()))
}

/// The frame's name as the panel's context says it (the web's).
pub fn frame_name(frame: &str) -> &'static str {
    match frame.split(':').next().unwrap_or("") {
        "tab" => "tab", "cell" => "cell", "inline" => "inline", "dock" => "dock", "chip" => "chipbar", "pop" => "chip", _ => "cell",
    }
}

/// The title channel's prefix (an invisible separator, then "wg<load>").
const TITLE_MARK: char = '\u{2063}';

/// The messages a page wrote to its title for load `nonce`: (seq, message).
pub fn parse_title(title: &str, nonce: u64) -> Option<Vec<(u64, Value)>> {
    let rest = title.strip_prefix(TITLE_MARK)?.strip_prefix("wg")?;
    let (n, body) = rest.split_once(' ')?;
    if n.parse::<u64>().ok()? != nonce { return None; }
    let arr: Vec<Value> = serde_json::from_str(body).ok()?;
    Some(arr.into_iter().filter_map(|e| Some((e.get(0)?.as_u64()?, e.get(1)?.clone()))).collect())
}

/// JSON for inside a <script>: no "</" that could end it.
fn script_json(v: &Value) -> String { v.to_string().replace("</", "<\\/").replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029") }

/// The page for a view: the server's document with the bridge ahead of the
/// runtime (see the top of this file). `init` is the runtime's first message.
pub fn build_html(doc: &str, nonce: u64, init: &Value) -> String {
    let shim = format!(
        r#"<script data-crundi>(function(){{var N={nonce},I={init},q=0,out=[],real=window.postMessage.bind(window);
function flush(){{var s=JSON.stringify(out);while(s.length>3000&&out.length>1){{out.shift();s=JSON.stringify(out);}}document.title='\u2063wg'+N+' '+s;}}
function give(m){{m.crundi=1;m.__in=1;window.postMessage(m,'*');}}
window.postMessage=function(m,o){{
if(m&&m.crundi===1&&m.__in){{delete m.__in;return real(m,'*');}}
if(m&&m.crundi===1){{
if(m.t==='ready'){{setTimeout(function(){{give(I);setTimeout(function(){{give({{t:'measure'}});}},200);}},0);}}
if(m.t==='height')out=out.filter(function(x){{return x[1].t!=='height';}});
if(m.t==='lint')return;
out.push([++q,m]);if(out.length>12)out.shift();flush();return;}}
return real(m,o);}};}})();</script>"#,
        nonce = nonce, init = script_json(init)
    );
    match doc.find("<head>") {
        Some(i) => format!("{}{}{}", &doc[..i + 6], shim, &doc[i + 6..]),
        None => format!("{shim}{doc}"),
    }
}

// ─── State ───

struct Grant { hash: String, items: Vec<(String, String)> }

#[derive(Default)]
struct View {
    frame: String,
    alias: String,
    id: String,
    /// Each load of the page; the title channel names it.
    nonce: u64,
    loading: bool,
    /// "" not yet, "loading", "error", "ready".
    state: String,
    error: String,
    grant: Option<Grant>,
    grant_hidden: bool,
    approving: bool,
    height: f32,
    measured: bool,
    chip_w: f32,
    title: String,
    last_seq: u64,
    faults: u32,
    calls: Vec<Instant>,
    store_call: Option<Instant>,
    size: (f32, f32),
    seen: Option<Instant>,
    handed: Option<Instant>,
    reload_at: Option<Instant>,
    msgs: u64,
}

#[derive(Default)]
struct St {
    project: String,
    signed: bool,
    widgets: Vec<Value>,
    loaded: bool,
    error: String,
    seq: u64,
    selected: String,
    pane: String,
    versions: Vec<Value>,
    versions_state: String,
    shots: Vec<(String, Option<slint::Image>)>,
    checks_state: String,
    checks_seq: u64,
    picked: HashMap<String, String>,
    chip_values: HashMap<String, String>,
    pop: String,
    views: HashMap<String, View>,
    cells_sig: String,
    on_cells: Vec<Rc<dyn Fn()>>,
    opened: String,
    last_call: Value,
    ticks: u64,
}

thread_local! {
    static ST: RefCell<St> = RefCell::new(St::default());
    static MODELS: RefCell<Option<Models>> = const { RefCell::new(None) };
}

struct Models {
    items: Rc<VecModel<crate::OpsPanelItem>>,
    versions: Rc<VecModel<crate::OpsPanelVersion>>,
    shots: Rc<VecModel<crate::OpsPanelShot>>,
    picks: Rc<VecModel<crate::OpsPanelPick>>,
    chips: Rc<VecModel<crate::OpsPanelChip>>,
}

fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { ST.with(|s| f(&mut s.borrow_mut())) }

fn ui() -> Option<MainWindow> { kit::ui() }

fn bump() { if let Some(w) = ui() { let g = w.global::<crate::OpsPanels>(); g.set_gen(g.get_gen().wrapping_add(1)); } }

fn sv(v: Vec<String>) -> ModelRc<SharedString> { ModelRc::new(VecModel::from(v.into_iter().map(SharedString::from).collect::<Vec<_>>())) }

fn by_id(s: &St, id: &str) -> Option<Value> { s.widgets.iter().find(|w| kit::s(w, "id") == id).cloned() }

/// The chats of the project on screen (a docked panel needs its chat).
fn live_chats(project: &str) -> Vec<String> {
    kit::terminals().unwrap_or_default().iter()
        .filter(|t| kit::s(t, "kind") == "ui" && kit::s(t, "project").eq_ignore_ascii_case(project))
        .map(|t| kit::s(t, "id")).collect()
}

fn ok_of(v: &Value) -> bool { v.get("ok").is_some_and(kit::truthy) }

// ─── Public: for the workspace area (panes, docks, inline cards) ───

/// The ids of the panels that are workbench panes now ("widget:<id>").
pub fn cell_ids() -> Vec<String> {
    let (list, p) = with(|s| (s.widgets.clone(), s.project.clone()));
    if p.is_empty() { return vec![]; }
    let live = live_chats(&p);
    list.iter().filter(|w| place_of(w, &live) == "cell").map(|w| kit::s(w, "id")).collect()
}

/// A panel's title (a pane's header).
pub fn title_of(id: &str) -> String { with(|s| by_id(s, id).map(|w| kit::s(&w, "title")).unwrap_or_default()) }

/// Is a panel waiting for the owner's approval (a pane header's "approve")?
pub fn needs_grant(id: &str) -> bool { with(|s| by_id(s, id).is_some_and(|w| w.get("needsGrant").is_some_and(kit::truthy))) }

/// Called whenever the set of panel panes changes (rebuild the panes).
pub fn on_cells_changed(f: impl Fn() + 'static) { with(|s| s.on_cells.push(Rc::new(f))); }

/// A chooser pane ("panel:widget") whose panel is now a pane: (its key,
/// the panel's id), taken once; give that pane the key "widget:<id>".
pub fn take_swaps() -> Vec<(String, String)> {
    let cells = cell_ids();
    with(|s| {
        let done: Vec<(String, String)> = s.picked.iter().filter(|(_, id)| cells.contains(id)).map(|(k, id)| (k.clone(), id.clone())).collect();
        for (k, _) in &done { s.picked.remove(k); }
        done
    })
}

/// Does a tool call show an inline card under it (widget_open, slot inline)?
pub fn inline_card(tool: &str, input: &Value) -> bool { tool.ends_with("widget_open") && kit::s(input, "slot") == "inline" && !kit::s(input, "id").is_empty() }

/// A pane header's Reload and × (the web's widget-reload, widget-close).
pub fn reload(id: &str) { reload_id(id); }
pub fn close(id: &str) { user_patch(id, json!({ "closed": true }), |_| {}); }

/// A pane header's slot control: the slots as a menu at (x, y).
pub fn slot_menu(id: &str, x: f32, y: f32) {
    let Some(w) = with(|s| by_id(s, id)) else { return };
    let (labels, values, cur) = slot_options(&w);
    let items = labels.iter().zip(values.iter()).enumerate().map(|(i, (l, v))| kit::item(v, &(if i as i32 == cur { format!("{l} ✓") } else { l.clone() }))).collect();
    let id = id.to_string();
    kit::menu(x, y, items, move |v| if !v.is_empty() { user_patch(&id, json!({ "slot": v }), |_| {}); });
}

// ─── The list ───

fn refresh() {
    let (p, seq) = with(|s| { s.seq += 1; (s.project.clone(), s.seq) });
    if kit::api().is_none() { return; }
    kit::call(move |api| api.get(&format!("/api/widgets?project={}", kit::enc(&p))), move |r| {
        if with(|s| s.seq != seq) { return; }
        with(|s| match r {
            Ok(v) => {
                s.widgets = if ok_of(&v) { v.get("widgets").and_then(|x| x.as_array()).cloned().unwrap_or_default() } else { vec![] };
                s.loaded = true;
                s.error.clear();
            }
            Err(e) => { s.error = e; }   // shown (with Retry) unless panels are on screen already
        });
        after_list();
    });
}

/// The list changed: the selection, the chooser's picks, chips, panes.
fn after_list() {
    let (p, list) = with(|s| (s.project.clone(), s.widgets.clone()));
    let live = live_chats(&p);
    with(|s| {
        let order = tab_order(&s.widgets);
        if by_id(s, &s.selected).is_none() { s.selected = order.first().map(|w| kit::s(w, "id")).unwrap_or_default(); s.pane.clear(); }
        // A chooser whose panel left the workbench shows the list again.
        let gone: Vec<String> = s.picked.iter().filter(|(_, id)| by_id(s, id).is_none_or(|w| kit::s(&w, "state") != "open")).map(|(k, _)| k.clone()).collect();
        for k in gone { s.picked.remove(&k); }
        if !s.pop.is_empty() && by_id(s, &s.pop).is_none() { s.pop.clear(); }
    });
    // Chip values.
    for w in list.iter().filter(|w| place_of(w, &live) == "chip") { load_chip_value(w); }
    push();
    let sig = cell_ids().join(",");
    let cbs = with(|s| if s.cells_sig != sig { s.cells_sig = sig; s.on_cells.clone() } else { vec![] });
    for f in cbs { f(); }
}

fn load_chip_value(w: &Value) {
    let Some(chip) = w.get("chip").filter(|c| c.is_object()) else { return };
    let source = kit::s(chip, "source");
    if source.is_empty() { return; }
    let path = kit::s(chip, "path");
    let id = kit::s(w, "id");
    let p = with(|s| s.project.clone());
    let q = format!("/api/widgets/data?project={}&id={}&source={}", kit::enc(&p), kit::enc(&id), kit::enc(&source));
    kit::call(move |api| api.get(&q), move |r| {
        let Ok(d) = r else { return };
        let entry = if ok_of(&d) { d.get("data").and_then(|x| x.get(&source)).and_then(|e| e.get("value")) } else { None };
        let text = chip_text(entry.and_then(|v| path_get(v, &path)));
        with(|s| s.chip_values.insert(id, text));
        push();
    });
}

/// Everything the screen shows, from the state.
fn push() {
    let Some(w) = ui() else { return };
    let g = w.global::<crate::OpsPanels>();
    let (p, list, loaded, error, selected, pane, chip_values, pop) = with(|s| (s.project.clone(), s.widgets.clone(), s.loaded, s.error.clone(), s.selected.clone(), s.pane.clone(), s.chip_values.clone(), s.pop.clone()));
    let live = live_chats(&p);
    g.set_project(p.clone().into());
    let state = if !error.is_empty() && (!loaded || list.is_empty()) { "error" } else if !loaded { "loading" } else if list.is_empty() { if p.is_empty() { "empty-global" } else { "empty" } } else { "list" };
    g.set_state(state.into());
    g.set_error(error.into());
    let items: Vec<crate::OpsPanelItem> = tab_order(&list).iter().map(|w| {
        let (badge, tone) = item_badge(w);
        crate::OpsPanelItem {
            id: kit::s(w, "id").into(), title: kit::s(w, "title").into(), sub: item_sub(w).into(), badge: badge.into(), tone: tone.into(),
            global: w.get("global").is_some_and(kit::truthy), closed: kit::s(w, "state") != "open", selected: kit::s(w, "id") == selected,
        }
    }).collect();
    let sel = list.iter().find(|w| kit::s(w, "id") == selected).map(|w| {
        let (labels, values, i) = slot_options(w);
        crate::OpsPanelSel {
            id: kit::s(w, "id").into(), title: kit::s(w, "title").into(), open: kit::s(w, "state") == "open", pinned: kit::s(w, "lifecycle") == "pinned",
            scope: scope_of(w, &p).into(), home: kit::s(w, "home").into(), slot_labels: sv(labels), slot_values: sv(values), slot_index: i,
        }
    }).unwrap_or_default();
    let mut by_new = list.clone();
    by_new.sort_by(|a, b| kit::n(b, "updatedAt").total_cmp(&kit::n(a, "updatedAt")));
    let picks: Vec<crate::OpsPanelPick> = by_new.iter().map(|w| {
        let place = place_of(w, &live);
        crate::OpsPanelPick { id: kit::s(w, "id").into(), title: kit::s(w, "title").into(), status: pick_status(w, place).into(), here: place == "cell" }
    }).collect();
    let chips: Vec<crate::OpsPanelChip> = list.iter().filter(|w| place_of(w, &live) == "chip").map(|w| {
        let id = kit::s(w, "id");
        let grant = w.get("needsGrant").is_some_and(kit::truthy);
        let label = w.get("chip").map(|c| kit::s(c, "label")).filter(|l| !l.is_empty()).unwrap_or_else(|| kit::s(w, "title"));
        crate::OpsPanelChip {
            id: id.clone().into(), label: label.into(), value: chip_values.get(&id).cloned().unwrap_or_default().into(),
            title: format!("{}{}", kit::s(w, "title"), if grant { " (waiting for your approval)" } else { "" }).into(),
            approve: grant, live: w.get("chipLive").is_some_and(kit::truthy) && !grant,
        }
    }).collect();
    MODELS.with(|m| if let Some(m) = &*m.borrow() {
        kit::sync_rows(&m.items, items);
        kit::sync_rows(&m.picks, picks);
        kit::sync_rows(&m.chips, chips);
    });
    g.set_sel(sel);
    g.set_pane(pane.into());
    let popw = list.iter().find(|w| kit::s(w, "id") == pop).cloned();
    g.set_pop_open(popw.is_some());
    g.set_modal(popw.is_some());
    if let Some(pw) = popw {
        let (labels, values, i) = slot_options(&pw);
        g.set_pop_id(pop.into());
        g.set_pop_title(kit::s(&pw, "title").into());
        g.set_pop_slot_labels(sv(labels));
        g.set_pop_slot_values(sv(values));
        g.set_pop_slot_index(i);
    }
    push_panes();
    bump();
}

fn push_panes() {
    let Some(w) = ui() else { return };
    let g = w.global::<crate::OpsPanels>();
    let (vs, vstate, shots, cstate) = with(|s| (s.versions.clone(), s.versions_state.clone(), s.shots.clone(), s.checks_state.clone()));
    let n = vs.len();
    let versions: Vec<crate::OpsPanelVersion> = vs.iter().enumerate().map(|(i, v)| crate::OpsPanelVersion {
        version: kit::s(v, "version").into(), when: when(kit::n(v, "at")).into(), current: i == 0,
    }).collect();
    let _ = n;
    let shots: Vec<crate::OpsPanelShot> = shots.into_iter().map(|(c, img)| crate::OpsPanelShot { caption: c.into(), ready: img.is_some(), image: img.unwrap_or_default() }).collect();
    MODELS.with(|m| if let Some(m) = &*m.borrow() {
        m.versions.set_vec(versions);
        m.shots.set_vec(shots);
    });
    g.set_history_state(vstate.into());
    g.set_checks_state(cstate.into());
}

/// A version's time as the web's toLocaleString.
fn when(ms: f64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(ms as i64).single().map(|t| t.format("%d/%m/%Y, %H:%M:%S").to_string()).unwrap_or_default()
}

// ─── The tab ───

fn select(id: &str) {
    with(|s| { s.selected = id.to_string(); s.pane.clear(); });
    push();
}

fn show_pane(pane: &str) {
    let (p, id) = with(|s| { s.pane = pane.to_string(); (s.project.clone(), s.selected.clone()) });
    if pane == "history" {
        with(|s| { s.versions.clear(); s.versions_state = "loading".into(); });
        let q = format!("/api/widgets/versions?project={}&id={}", kit::enc(&p), kit::enc(&id));
        kit::call(move |api| api.get(&q), move |r| {
            let list = r.ok().filter(ok_of).and_then(|d| d.get("versions").and_then(|x| x.as_array()).cloned()).unwrap_or_default();
            with(|s| { s.versions_state = if list.is_empty() { "empty".into() } else { "list".into() }; s.versions = list; });
            push_panes();
        });
    }
    if pane == "checks" { load_shots(); }
    push();
}

/// The screenshots of Claude's last check (the web's shotsNode).
fn load_shots() {
    let (p, id, seq) = with(|s| { s.checks_seq += 1; s.shots.clear(); s.checks_state = "loading".into(); (s.project.clone(), s.selected.clone(), s.checks_seq) });
    let q = format!("/api/widgets/renders?project={}&id={}", kit::enc(&p), kit::enc(&id));
    kit::call(move |api| api.get(&q), move |r| {
        if with(|s| s.checks_seq != seq) { return; }
        let renders = r.ok().filter(ok_of).and_then(|d| d.get("renders").cloned()).unwrap_or(Value::Null);
        let shots = renders.get("shots").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let at = kit::s(&renders, "at");
        with(|s| {
            s.checks_state = if shots.is_empty() { "none".into() } else { "list".into() };
            s.shots = shots.iter().map(|sh| {
                let (f, st) = (kit::s(sh, "frame"), kit::s(sh, "state"));
                (if st != "live" { format!("{f} · {st}") } else { f }, None)
            }).collect();
        });
        push_panes();
        let Some(api) = kit::api() else { return };
        for (i, sh) in shots.iter().enumerate() {
            let name = format!("{}--{}", kit::s(sh, "frame"), kit::s(sh, "state"));
            let path = format!("/api/widgets/render-image?project={}&id={}&name={}&t={}", kit::enc(&p), kit::enc(&id), kit::enc(&name), kit::enc(&at));
            let api = api.clone();
            kit::bg(move || api.get_bytes(&path).ok().filter(|(st, _)| *st == 200).and_then(|(_, b)| image::load_from_memory(&b).ok()).map(|img| {
                let rgba = img.to_rgba8();
                slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba.as_raw(), rgba.width(), rgba.height())
            }), move |pb| {
                let Some(pb) = pb else { return };
                if with(|s| s.checks_seq != seq) { return; }
                with(|s| if let Some(x) = s.shots.get_mut(i) { x.1 = Some(slint::Image::from_rgba8(pb)); });
                push_panes();
            });
        }
    });
}

fn user_patch(id: &str, patch: Value, then: impl FnOnce(bool) + Send + 'static) {
    let p = with(|s| s.project.clone());
    let body = json!({ "project": p, "id": id, "patch": patch });
    kit::call(move |api| api.post("/api/widgets/user", &body), move |r| {
        let ok = match r {
            Ok(v) if ok_of(&v) => true,
            Ok(v) => { kit::toast_error(&Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Could not update the panel".into())); false }
            Err(e) => { kit::toast_error(&e); false }
        };
        then(ok);
    });
}

fn set_scope(id: &str, global: bool) {
    let p = with(|s| s.project.clone());
    let body = json!({ "project": p, "id": id, "global": global });
    kit::call(move |api| api.post("/api/widgets/scope", &body), move |r| {
        let v = match r { Ok(v) => v, Err(e) => return kit::toast_error(&e) };
        if !ok_of(&v) {
            let e = kit::s(&v, "error");
            let msg = if !e.is_empty() { e } else if global { "Could not make it global".into() } else { "Could not change it".into() };
            return kit::toast_error(&msg);
        }
        if global {
            let n = v.get("inPlace").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0);
            kit::toast(&format!("Now global: shown in every project{}", if n > 0 { format!(". It still reads {n} file{} from this project", if n == 1 { "" } else { "s" }) } else { String::new() }));
        } else {
            kit::toast("Back in this project only");
        }
    });
}

fn delete(id: &str) {
    let Some(w) = with(|s| by_id(s, id)) else { return };
    let (p, id) = (with(|s| s.project.clone()), id.to_string());
    let msg = format!("Delete \"{}\" and its source files? This cannot be undone.", kit::s(&w, "title"));
    kit::confirm(kit::Confirm { title: "Delete panel", message: &msg, ok: "Delete", danger: true }, move |ok| {
        if !ok { return; }
        let body = json!({ "project": p, "id": id });
        kit::call(move |api| api.post("/api/widgets/delete", &body), |r| match r {
            Ok(v) if !ok_of(&v) => kit::toast_error(&Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Could not delete".into())),
            Err(e) => kit::toast_error(&e),
            _ => {}
        });
    });
}

fn restore(id: &str, version: &str) {
    let p = with(|s| s.project.clone());
    let ver: Value = version.parse::<u64>().map(Value::from).unwrap_or_else(|_| Value::from(version));
    let body = json!({ "project": p, "id": id, "version": ver });
    let version = version.to_string();
    kit::call(move |api| api.post("/api/widgets/rollback", &body), move |r| match r {
        Ok(v) if ok_of(&v) => { kit::toast(&format!("Restored v{version}")); show_pane(""); }
        Ok(v) => kit::toast_error(&Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Could not restore".into())),
        Err(e) => kit::toast_error(&e),
    });
}

/// Reload: every copy of the panel in this project loads again.
fn reload_id(id: &str) {
    let pk = with(|s| pkey(&s.project));
    let keys: Vec<String> = with(|s| s.views.iter().filter(|(_, v)| v.id == id && pkey(&v.alias) == pk).map(|(k, _)| k.clone()).collect());
    for k in keys { load(&k); }
}

// ─── Views ───

/// The frames.rs entry of a view (live, painted, title), if it has one.
fn frame_entry(key: &str) -> Option<Value> {
    crate::frames::list().as_array()?.iter().find(|f| kit::s(f, "key") == key).cloned()
}

/// A box reported itself: keep its view alive, load it, read its messages.
fn seen(key: &str, w: f32, h: f32, ax: f32, ay: f32) {
    let Some((frame, alias, id)) = parse_key(key) else { return };
    if kit::api().is_none() { return; }
    let first = with(|s| {
        let v = s.views.entry(key.to_string()).or_insert_with(|| View { frame: frame.clone(), alias: alias.clone(), id: id.clone(), ..Default::default() });
        v.seen = Some(Instant::now());
        v.size = (w, h);
        v.state.is_empty()
    });
    let scale = ui().map(|u| u.window().scale_factor()).unwrap_or(1.0);
    let (k, target) = (key.to_string(), format!("widget:{alias}/{id}"));
    crate::browser::defer(move || crate::frames::seen(&k, "", &target, w, h, ax, ay, scale, true, 0.0));
    if first { return load(key); }
    let entry = frame_entry(key);
    let live = entry.as_ref().is_some_and(|e| e.get("live").is_some_and(kit::truthy));
    // Released (scrolled away, the cap, a sweep) or never made: again, fresh.
    let again = with(|s| s.views.get(key).is_some_and(|v| v.state == "ready" && !v.loading && !live && v.handed.is_none_or(|t| t.elapsed() > Duration::from_millis(1500))));
    if again { return load(key); }
    if let Some(title) = entry.as_ref().map(|e| kit::s(e, "title")) { read_title(key, &title); }
}

/// Load (or reload) a view's page: its document and data, then the page.
fn load(key: &str) {
    let Some((alias, id, frame, nonce)) = with(|s| s.views.get_mut(key).map(|v| {
        v.loading = true;
        v.nonce += 1;
        v.reload_at = None;
        if v.state.is_empty() { v.state = "loading".into(); }
        (v.alias.clone(), v.id.clone(), v.frame.clone(), v.nonce)
    })) else { return };
    bump();
    let chip = frame == "chip";
    let doc_q = format!("/api/widgets/doc?project={}&id={}{}", kit::enc(&alias), kit::enc(&id), if chip { "&chip=1" } else { "" });
    let data_q = format!("/api/widgets/data?project={}&id={}", kit::enc(&alias), kit::enc(&id));
    let key = key.to_string();
    kit::call(move |api| {
        let doc = match api.get(&doc_q) {
            Ok(d) => d,
            // "No such widget" comes as a 404 with the server's words.
            Err(ApiError::Status(404, m)) if !m.is_empty() => json!({ "ok": false, "error": m }),
            Err(e) => return Err(e),
        };
        let data = if ok_of(&doc) { api.get(&data_q).unwrap_or(Value::Null) } else { Value::Null };
        Ok(json!({ "doc": doc, "data": data }))
    }, move |r| loaded(&key, nonce, r));
}

fn loaded(key: &str, nonce: u64, r: Result<Value, String>) {
    let mut hand: Option<String> = None;
    let stale = with(|s| {
        let Some(v) = s.views.get_mut(key) else { return true };
        if v.nonce != nonce { return true; }
        v.loading = false;
        match &r {
            Err(e) => { v.state = "error".into(); v.error = e.clone(); }
            Ok(d) => {
                let doc = d.get("doc").cloned().unwrap_or(Value::Null);
                if !ok_of(&doc) {
                    v.state = "error".into();
                    v.error = Some(kit::s(&doc, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "This panel could not be loaded.".into());
                } else {
                    v.state = "ready".into();
                    v.error.clear();
                    v.title = doc.get("widget").map(|w| kit::s(w, "title")).unwrap_or_default();
                    let g = doc.get("grant").cloned().unwrap_or(Value::Null);
                    v.grant = (g.get("needed").is_some_and(kit::truthy) && !g.get("granted").is_some_and(kit::truthy)).then(|| Grant {
                        hash: kit::s(&g, "hash"),
                        items: g.get("items").and_then(|x| x.as_array()).map(|a| a.iter().map(|i| (kit::s(i, "what"), kit::s(i, "detail"))).collect()).unwrap_or_default(),
                    });
                    v.grant_hidden = false;
                    v.approving = false;
                    v.last_seq = 0;
                    v.faults = 0;
                    let data = d.get("data").cloned().unwrap_or(Value::Null);
                    let ok = ok_of(&data);
                    let init = json!({
                        "t": "init",
                        "data": if ok { data.get("data").cloned().unwrap_or(json!({})) } else { json!({}) },
                        "store": if ok { data.get("store").cloned().filter(|x| x.is_object()).unwrap_or(json!({})) } else { json!({}) },
                        "context": { "frame": frame_name(&v.frame), "platform": "desktop", "touch": false, "width": v.size.0.round(), "height": v.size.1.round() },
                    });
                    hand = Some(build_html(&kit::s(&doc, "html"), nonce, &init));
                    v.handed = Some(Instant::now());
                }
            }
        }
        false
    });
    if stale { return; }
    if let Some(html) = hand {
        let k = key.to_string();
        crate::browser::defer(move || {
            crate::frames::release(&k);
            crate::frames::fetched(&k, Some(html.into_bytes()));
        });
    }
    bump();
}

/// The page's messages, from its title (see the top of this file).
fn read_title(key: &str, title: &str) {
    let Some(nonce) = with(|s| s.views.get(key).map(|v| v.nonce)) else { return };
    let Some(msgs) = parse_title(title, nonce) else { return };
    let fresh: Vec<Value> = with(|s| {
        let Some(v) = s.views.get_mut(key) else { return vec![] };
        let out: Vec<Value> = msgs.iter().filter(|(q, _)| *q > v.last_seq).map(|(_, m)| m.clone()).collect();
        if let Some((q, _)) = msgs.last() { v.last_seq = v.last_seq.max(*q); }
        v.msgs += out.len() as u64;
        out
    });
    for m in fresh { on_message(key, &m); }
}

fn on_message(key: &str, m: &Value) {
    match kit::s(m, "t").as_str() {
        "height" => {
            with(|s| if let Some(v) = s.views.get_mut(key) {
                v.height = kit::n(m, "h") as f32;
                v.measured = true;
                if m.get("w").is_some() { v.chip_w = kit::n(m, "w") as f32; }
            });
            bump();
        }
        "fault" => {
            let Some((alias, id, n)) = with(|s| s.views.get_mut(key).map(|v| { v.faults += 1; (v.alias.clone(), v.id.clone(), v.faults) })) else { return };
            // A handful is a diagnosis; a flood is a loop.
            if n > 5 { return; }
            let body = json!({ "project": alias, "id": id, "message": kit::s(m, "message").chars().take(500).collect::<String>(), "where": kit::s(m, "where").chars().take(120).collect::<String>() });
            kit::call(move |api| api.post("/api/widgets/fault", &body), |_| {});
        }
        "call" => on_call(key, m),
        _ => {}
    }
}

/// Something done inside a panel: toast and openLink here, the rest by the server.
fn on_call(key: &str, m: &Value) {
    let Some((alias, id, ok)) = with(|s| s.views.get_mut(key).map(|v| {
        let now = Instant::now();
        v.calls.retain(|t| now.duration_since(*t) < Duration::from_secs(10));
        let ok = v.calls.len() < 40;
        if ok { v.calls.push(now); }
        (v.alias.clone(), v.id.clone(), ok)
    })) else { return };
    if !ok { return; }
    let op = kit::s(m, "op");
    let a = m.get("args").cloned().unwrap_or(Value::Null);
    match op.as_str() {
        "toast" => {
            let text: String = kit::s(&a, "text").chars().take(200).collect();
            if kit::s(&a, "kind") == "error" { kit::toast_error(&text); } else { kit::toast(&text); }
            with(|s| s.last_call = json!({ "op": op, "ok": true }));
        }
        "openLink" => {
            let url = kit::s(&a, "url");
            let lower = url.to_lowercase();
            // http(s) only: never javascript: or data: in a real browser.
            if lower.starts_with("http://") || lower.starts_with("https://") {
                with(|s| s.opened = url.clone());
                if std::env::var_os("CRUNDI_TEST_PORT").is_none() { let _ = open::that(&url); }
            }
            with(|s| s.last_call = json!({ "op": op, "url": url }));
        }
        _ => {
            if op == "store.set" { with(|s| if let Some(v) = s.views.get_mut(key) { v.store_call = Some(Instant::now()); }); }
            let body = json!({ "project": alias, "id": id, "op": op, "args": a });
            let op2 = op.clone();
            kit::call(move |api| api.post("/api/widgets/call", &body), move |r| {
                let res = match r { Ok(v) => v, Err(e) => json!({ "ok": false, "error": e }) };
                with(|s| s.last_call = json!({ "op": op2, "result": res }));
            });
        }
    }
}

fn approve(key: &str) {
    let Some((alias, id, hash)) = with(|s| s.views.get_mut(key).and_then(|v| { v.approving = true; v.grant.as_ref().map(|g| (v.alias.clone(), v.id.clone(), g.hash.clone())) })) else { return };
    bump();
    let body = json!({ "project": alias, "id": id, "hash": hash });
    let key = key.to_string();
    kit::call(move |api| api.post("/api/widgets/grant", &body), move |r| {
        let v = r.unwrap_or_else(|e| json!({ "ok": false, "error": e }));
        if ok_of(&v) { return kit::toast("Approved"); }
        kit::toast_error(&Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Could not approve".into()));
        if v.get("stale").is_some_and(kit::truthy) { load(&key); } else { with(|s| if let Some(v) = s.views.get_mut(&key) { v.approving = false; }); bump(); }
    });
}

fn frame_info(key: &str) -> crate::OpsPanelFrame {
    with(|s| {
        let Some(v) = s.views.get(key) else { return crate::OpsPanelFrame { state: "loading".into(), ..Default::default() } };
        let items: Vec<crate::OpsPanelGrantItem> = v.grant.as_ref().map(|g| g.items.iter().map(|(w, d)| crate::OpsPanelGrantItem { what: w.into(), detail: d.into() }).collect()).unwrap_or_default();
        crate::OpsPanelFrame {
            state: (if v.state.is_empty() { "loading" } else { v.state.as_str() }).into(), error: v.error.clone().into(),
            grant: v.grant.is_some() && !v.grant_hidden, grant_items: ModelRc::new(VecModel::from(items)), approving: v.approving,
            height: v.height, measured: v.measured, title: v.title.clone().into(), chip_w: v.chip_w,
        }
    })
}

fn docks(chat: &str) -> Vec<crate::OpsPanelDock> {
    let (p, list) = with(|s| (s.project.clone(), s.widgets.clone()));
    let live = live_chats(&p);
    let mut live = live;
    // The chat asking is on screen, whatever the last state said.
    if !live.iter().any(|c| c == chat) { live.push(chat.to_string()); }
    list.iter().filter(|w| kit::s(w, "chat") == chat && place_of(w, &live) == "dock").map(|w| {
        let (labels, values, i) = slot_options(w);
        crate::OpsPanelDock {
            id: kit::s(w, "id").into(), title: kit::s(w, "title").into(), approve: w.get("needsGrant").is_some_and(kit::truthy),
            collapsed: w.get("collapsed").is_some_and(kit::truthy), slot_labels: sv(labels), slot_values: sv(values), slot_index: i,
        }
    }).collect()
}

fn picked_of(key: &str) -> String {
    if let Some(id) = key.strip_prefix("widget:") { return id.to_string(); }
    with(|s| s.picked.get(key).cloned().unwrap_or_default())
}

fn pick(key: &str, id: &str) {
    with(|s| s.picked.insert(key.to_string(), id.to_string()));
    bump();
    let (k, i) = (key.to_string(), id.to_string());
    user_patch(id, json!({ "closed": false, "slot": "cell" }), move |ok| {
        if !ok { with(|s| if s.picked.get(&k) == Some(&i) { s.picked.remove(&k); }); bump(); }
    });
}

fn collapse(id: &str, c: bool) {
    with(|s| if let Some(w) = s.widgets.iter_mut().find(|w| kit::s(w, "id") == id) { w["collapsed"] = Value::Bool(c); });
    bump();
    user_patch(id, json!({ "collapsed": c }), |_| {});
}

fn open_pop(id: &str) { with(|s| s.pop = id.to_string()); push(); }
fn close_pop() { with(|s| s.pop.clear()); push(); }

// ─── Live events ───

fn on_widget_event(d: &Value) {
    let project = kit::s(d, "project");
    let g = project == "_global";
    let mine = with(|s| pkey(&s.project));
    if !g && project != mine { return; }
    let id = kit::s(d, "id");
    let kind = kit::s(d, "kind");
    let matching = |skip_store: bool| -> Vec<String> {
        with(|s| s.views.iter_mut().filter(|(_, v)| v.id == id && (g || pkey(&v.alias) == project)).filter_map(|(k, v)| {
            if skip_store && v.store_call.is_some_and(|t| t.elapsed() < Duration::from_secs(3)) { return None; }
            Some(k.clone())
        }).collect())
    };
    match kind.as_str() {
        "data" => {
            // New values: the page loads again with them (no way in yet).
            let keys = matching(false);
            with(|s| for k in &keys { if let Some(v) = s.views.get_mut(k) { v.reload_at.get_or_insert(Instant::now() + Duration::from_millis(300)); } });
            let source = kit::s(d, "source");
            let w = with(|s| by_id(s, &id));
            let live = live_chats(&with(|s| s.project.clone()));
            if let Some(w) = w.filter(|w| place_of(w, &live) == "chip") {
                let cs = w.get("chip").map(|c| kit::s(c, "source")).unwrap_or_default();
                if source.is_empty() || cs.is_empty() || cs == source { load_chip_value(&w); }
            }
        }
        "store" => {
            let keys = matching(true);
            with(|s| for k in &keys { if let Some(v) = s.views.get_mut(k) { v.reload_at.get_or_insert(Instant::now() + Duration::from_millis(300)); } });
        }
        "doc" => {
            for k in matching(false) { load(&k); }
            refresh();
        }
        "render" => {
            if with(|s| s.pane == "checks" && s.selected == id) { load_shots(); }
        }
        _ => {
            refresh();
            if with(|s| s.selected == id && !s.pane.is_empty()) { let p = with(|s| s.pane.clone()); show_pane(&p); }
        }
    }
}

/// Every 250ms: due reloads, views no longer drawn forgotten; every 25s the
/// server hears which panels are on screen (their sources keep running).
fn tick() {
    let (due, watch) = with(|s| {
        s.ticks += 1;
        let now = Instant::now();
        let due: Vec<String> = s.views.iter().filter(|(_, v)| v.reload_at.is_some_and(|t| t <= now) && !v.loading).map(|(k, _)| k.clone()).collect();
        s.views.retain(|_, v| v.seen.is_none_or(|t| t.elapsed() < Duration::from_secs(30)));
        let watch = s.ticks % 100 == 0;
        (due, watch)
    });
    for k in due {
        let drawn = with(|s| s.views.get(&k).is_some_and(|v| v.seen.is_some_and(|t| t.elapsed() < Duration::from_secs(1))));
        if drawn { load(&k); } else { with(|s| if let Some(v) = s.views.get_mut(&k) { v.reload_at = None; v.state = "ready".into(); v.handed = None; }); }
    }
    if watch { keep_alive(); }
}

fn keep_alive() {
    let (p, list, by) = with(|s| {
        let mut by: HashMap<String, Vec<String>> = HashMap::new();
        for v in s.views.values().filter(|v| v.seen.is_some_and(|t| t.elapsed() < Duration::from_secs(2))) {
            let e = by.entry(v.alias.clone()).or_default();
            if !e.contains(&v.id) { e.push(v.id.clone()); }
        }
        (s.project.clone(), s.widgets.clone(), by)
    });
    let mut by = by;
    for w in list.iter().filter(|w| kit::s(w, "state") == "open" && kit::s(w, "slot") == "chip") {
        let e = by.entry(p.clone()).or_default();
        let id = kit::s(w, "id");
        if !e.contains(&id) { e.push(id); }
    }
    for (alias, ids) in by {
        let body = json!({ "project": alias, "ids": ids });
        kit::call(move |api| api.post("/api/widgets/watch", &body), |_| {});
    }
}

// ─── The registry ───

pub fn init(w: &MainWindow) {
    let g = w.global::<crate::OpsPanels>();
    let m = Models { items: kit::model(), versions: kit::model(), shots: kit::model(), picks: kit::model(), chips: kit::model() };
    g.set_items(kit::rc(&m.items));
    g.set_versions(kit::rc(&m.versions));
    g.set_shots(kit::rc(&m.shots));
    g.set_picks(kit::rc(&m.picks));
    g.set_chips(kit::rc(&m.chips));
    MODELS.with(|x| *x.borrow_mut() = Some(m));
    g.on_frame(|k, _| frame_info(k.as_str()));
    g.on_picked_of(|k, _| picked_of(k.as_str()).into());
    g.on_docks(|c, _| ModelRc::new(VecModel::from(docks(c.as_str()))));
    g.on_title_of(|id, _| title_of(id.as_str()).into());
    g.on_seen(|k, w, h, x, y| seen(k.as_str(), w, h, x, y));
    g.on_select(|id| select(id.as_str()));
    g.on_show_pane(|p| show_pane(p.as_str()));
    g.on_slot(|id, v| {
        let in_pop = with(|s| s.pop == id.as_str());
        user_patch(id.as_str(), json!({ "slot": v.as_str() }), move |_| if in_pop { close_pop(); });
    });
    g.on_pin(|id| {
        let pinned = with(|s| by_id(s, id.as_str()).is_some_and(|w| kit::s(&w, "lifecycle") == "pinned"));
        user_patch(id.as_str(), json!({ "pinned": !pinned }), |_| {});
    });
    g.on_scope(|id, glob| set_scope(id.as_str(), glob));
    g.on_reload(|id| { show_pane(""); reload_id(id.as_str()); });
    g.on_close(|id| close(id.as_str()));
    g.on_reopen(|id| user_patch(id.as_str(), json!({ "closed": false }), |_| {}));
    g.on_delete(|id| delete(id.as_str()));
    g.on_restore(|id, v| restore(id.as_str(), v.as_str()));
    g.on_approve(|k| approve(k.as_str()));
    g.on_not_now(|k| { with(|s| if let Some(v) = s.views.get_mut(k.as_str()) { v.grant_hidden = true; }); bump(); });
    g.on_pick(|k, id| pick(k.as_str(), id.as_str()));
    g.on_open_tab(|id| { with(|s| { s.selected = id.to_string(); s.pane.clear(); }); kit::show_tab("panels"); push(); });
    g.on_chip(|id| open_pop(id.as_str()));
    g.on_pop_close(close_pop);
    g.on_collapse(|id, c| collapse(id.as_str(), c));
    g.on_retry(|| { with(|s| s.error.clear()); push(); refresh(); });
    let t = slint::Timer::default();
    t.start(slint::TimerMode::Repeated, Duration::from_millis(250), tick);
    std::mem::forget(t);
    push();
}

pub fn signed_in() {
    with(|s| { s.signed = true; s.project = kit::project(); });
    refresh();
}

pub fn signed_out() {
    let keys: Vec<String> = with(|s| {
        let keys = s.views.keys().cloned().collect();
        let cbs = std::mem::take(&mut s.on_cells);
        *s = St { on_cells: cbs, ..Default::default() };
        keys
    });
    crate::browser::defer(move || for k in keys { crate::frames::release(&k); });
    push();
}

pub fn project(alias: &str) {
    let changed = with(|s| {
        if s.project == alias && s.loaded { return false; }
        s.project = alias.to_string();
        // Global panels belong to every project: they stay while the list loads.
        s.widgets.retain(|w| w.get("global").is_some_and(kit::truthy));
        s.chip_values.clear();
        s.pop.clear();
        s.selected.clear();
        s.pane.clear();
        s.cells_sig = String::from("\u{0}");
        true
    });
    if changed { push(); refresh(); }
}

pub fn tab_shown(tab: &str) {
    if tab == "panels" { refresh(); }
}

pub fn event(e: &AreaEvent) {
    match e {
        AreaEvent::Server("widget", d) => on_widget_event(d),
        AreaEvent::Connected(true) => refresh(),
        _ => {}
    }
}

/// The driver's "ops:<name>" actions this screen knows (None: not mine).
pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let ok = json!({ "ok": true });
    Some(match name {
        "panels-refresh" => { refresh(); ok }
        "panels-select" => { select(&a(0)); ok }
        "panels-pane" => { show_pane(&a(0)); ok }
        "panels-slot" => { if let Some(w) = ui() { w.global::<crate::OpsPanels>().invoke_slot(a(0).into(), a(1).into()); } ok }
        "panels-pin" => { if let Some(w) = ui() { w.global::<crate::OpsPanels>().invoke_pin(a(0).into()); } ok }
        "panels-scope" => { set_scope(&a(0), a(1) == "true"); ok }
        "panels-reload" => { if let Some(w) = ui() { w.global::<crate::OpsPanels>().invoke_reload(a(0).into()); } ok }
        "panels-close" => { close(&a(0)); ok }
        "panels-reopen" => { user_patch(&a(0), json!({ "closed": false }), |_| {}); ok }
        "panels-delete" => { delete(&a(0)); ok }
        "panels-restore" => { restore(&a(0), &a(1)); ok }
        "panels-approve" => { approve(&a(0)); ok }
        "panels-not-now" => { if let Some(w) = ui() { w.global::<crate::OpsPanels>().invoke_not_now(a(0).into()); } ok }
        "panels-pick" => { pick(&a(0), &a(1)); ok }
        "panels-open-tab" => { if let Some(w) = ui() { w.global::<crate::OpsPanels>().invoke_open_tab(a(0).into()); } ok }
        "panels-chip" => { open_pop(&a(0)); ok }
        "panels-pop-close" => { close_pop(); ok }
        "panels-collapse" => { collapse(&a(0), a(1) == "true"); ok }
        "panels-slot-menu" => { slot_menu(&a(0), 400.0, 200.0); ok }
        // The driver's look at an inline card (project, id, chat; no id: hide).
        "panels-card" => {
            if let Some(w) = ui() {
                let g = w.global::<crate::OpsPanels>();
                g.set_preview_project(a(0).into()); g.set_preview_id(a(1).into()); g.set_preview_chat(a(2).into());
            }
            ok
        }
        "panels-cells" => json!({ "ok": true, "cells": cell_ids(), "swaps": with(|s| s.picked.clone()) }),
        _ => return None,
    })
}

/// This screen's part of the area's props (under "panels").
pub fn props() -> Value {
    let frames: HashMap<String, Value> = crate::frames::list().as_array().cloned().unwrap_or_default().into_iter().filter(|f| kit::s(f, "key").starts_with("wg|")).map(|f| (kit::s(&f, "key"), f)).collect();
    with(|s| {
        let views: serde_json::Map<String, Value> = s.views.iter().map(|(k, v)| {
            let f = frames.get(k);
            (k.clone(), json!({
                "state": v.state, "error": v.error, "grant": v.grant.is_some() && !v.grant_hidden, "grantItems": v.grant.as_ref().map(|g| g.items.len()).unwrap_or(0),
                "height": v.height, "measured": v.measured, "chipW": v.chip_w, "title": v.title, "loads": v.nonce, "msgs": v.msgs,
                "live": f.is_some_and(|f| f.get("live").is_some_and(kit::truthy)), "painted": f.is_some_and(|f| f.get("painted").is_some_and(kit::truthy)),
                "x": f.map(|f| kit::n(f, "x")).unwrap_or(0.0), "y": f.map(|f| kit::n(f, "y")).unwrap_or(0.0),
                "w": v.size.0, "h": v.size.1,
            }))
        }).collect();
        let live = live_chats(&s.project);
        json!({
            "project": s.project, "loaded": s.loaded, "error": s.error,
            "widgets": s.widgets.iter().map(|w| json!({ "id": kit::s(w, "id"), "title": kit::s(w, "title"), "slot": kit::s(w, "slot"), "state": kit::s(w, "state"), "place": place_of(w, &live), "global": w.get("global").is_some_and(kit::truthy), "needsGrant": w.get("needsGrant").is_some_and(kit::truthy), "lifecycle": kit::s(w, "lifecycle"), "collapsed": w.get("collapsed").is_some_and(kit::truthy) })).collect::<Vec<_>>(),
            "order": tab_order(&s.widgets).iter().map(|w| kit::s(w, "id")).collect::<Vec<_>>(),
            "selected": s.selected, "pane": s.pane,
            "versions": s.versions.iter().map(|v| kit::s(v, "version")).collect::<Vec<_>>(), "versionsState": s.versions_state,
            "shots": s.shots.iter().map(|x| json!({ "caption": x.0, "ready": x.1.is_some() })).collect::<Vec<_>>(), "checksState": s.checks_state,
            "picked": s.picked, "chipValues": s.chip_values, "pop": s.pop,
            "views": views, "opened": s.opened, "lastCall": s.last_call,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_names() {
        assert_eq!(pkey("Demo"), "demo");
        assert_eq!(pkey(""), "_global");
        assert_eq!(pkey("my proj!"), "my_proj_");
        assert_eq!(parse_key("wg|tab|demo|task-progress"), Some(("tab".into(), "demo".into(), "task-progress".into())));
        assert_eq!(parse_key("wg|inline:abc|demo|x"), Some(("inline:abc".into(), "demo".into(), "x".into())));
        assert_eq!(parse_key("wg|tab||x"), Some(("tab".into(), "".into(), "x".into())));
        assert_eq!(parse_key("live:x"), None);
        assert_eq!(frame_name("inline:abc"), "inline");
        assert_eq!(frame_name("chip"), "chipbar");
        assert_eq!(frame_name("pop"), "chip");
    }

    #[test]
    fn placement() {
        let live = vec!["c1".to_string()];
        let w = |slot: &str, chat: &str, state: &str| json!({ "slot": slot, "chat": chat, "state": state });
        assert_eq!(place_of(&w("dock", "", "open"), &live), "cell");
        assert_eq!(place_of(&w("dock", "c1", "open"), &live), "dock");
        assert_eq!(place_of(&w("dock", "c2", "open"), &live), "none");
        assert_eq!(place_of(&w("cell", "", "open"), &live), "cell");
        assert_eq!(place_of(&w("chip", "", "open"), &live), "chip");
        assert_eq!(place_of(&w("inline", "c1", "open"), &live), "none");
        assert_eq!(place_of(&w("tab", "", "open"), &live), "none");
        assert_eq!(place_of(&w("cell", "", "closed"), &live), "none");
        assert_eq!(pick_status(&w("cell", "", "open"), "cell"), "already open");
        assert_eq!(pick_status(&w("tab", "", "open"), "none"), "Panels tab");
        assert_eq!(pick_status(&w("tab", "", "closed"), "none"), "closed");
    }

    #[test]
    fn slots() {
        let (l, v, i) = slot_options(&json!({ "slot": "cell", "chat": "" }));
        assert_eq!(v, vec!["cell", "tab", "chip"]);
        assert_eq!(l[0], "Workbench pane");
        assert_eq!(i, 0);
        let (l, v, i) = slot_options(&json!({ "slot": "inline", "chat": "c" }));
        assert_eq!(v, vec!["inline", "dock", "cell", "tab", "chip"]);
        assert_eq!(l[0], "Inline in chat");
        assert_eq!(i, 0);
        assert_eq!(item_sub(&json!({ "state": "open", "slot": "dock" })), "docked");
        assert_eq!(item_sub(&json!({ "state": "open", "slot": "chip" })), "top");
        assert_eq!(item_sub(&json!({ "state": "closed", "slot": "chip" })), "closed");
        assert_eq!(item_badge(&json!({ "needsGrant": true, "faults": 2 })), ("approve", "warn"));
        assert_eq!(item_badge(&json!({ "faults": 2 })), ("error", "err"));
        assert_eq!(item_badge(&json!({})), ("", ""));
    }

    #[test]
    fn order_and_scope() {
        let list = vec![
            json!({ "id": "a", "state": "closed", "updatedAt": 30 }),
            json!({ "id": "b", "state": "open", "updatedAt": 10 }),
            json!({ "id": "c", "state": "open", "updatedAt": 20 }),
        ];
        let ids: Vec<String> = tab_order(&list).iter().map(|w| kit::s(w, "id")).collect();
        assert_eq!(ids, vec!["c", "b", "a"]);
        assert_eq!(scope_of(&json!({}), "demo"), "local");
        assert_eq!(scope_of(&json!({ "global": true, "home": "" }), "demo"), "global");
        assert_eq!(scope_of(&json!({ "global": true, "home": "demo" }), "Demo"), "global");
        assert_eq!(scope_of(&json!({ "global": true, "home": "other" }), "demo"), "other");
    }

    #[test]
    fn chip_values() {
        let v = json!({ "build": { "pct": 42, "name": "x" } });
        assert_eq!(chip_text(path_get(&v, "build.pct")), "42");
        assert_eq!(chip_text(path_get(&v, "build.none")), "");
        assert_eq!(chip_text(path_get(&json!("ok"), "")), "ok");
        assert_eq!(chip_text(Some(&json!("x".repeat(80)))).len(), 60);
    }

    #[test]
    fn bridge() {
        let html = build_html("<!doctype html><html><head><meta charset=\"utf-8\"></head><body></body></html>", 3, &json!({ "t": "init", "data": { "x": { "value": "</script>" } } }));
        let at = html.find("<script data-crundi>").unwrap();
        assert!(at < html.find("<meta").unwrap(), "the bridge comes before anything else");
        assert!(html.contains("N=3,"));
        assert!(!html.contains("\"</script>\""));
        let title = format!("{TITLE_MARK}wg3 [[1,{{\"t\":\"ready\",\"crundi\":1}}],[2,{{\"t\":\"height\",\"h\":120}}]]");
        let m = parse_title(&title, 3).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[1].0, 2);
        assert_eq!(kit::n(&m[1].1, "h"), 120.0);
        assert!(parse_title(&title, 4).is_none(), "an older load's title is not read");
        assert!(parse_title("Some page", 3).is_none());
        assert!(inline_card("mcp__crundi__widget_open", &json!({ "slot": "inline", "id": "x" })));
        assert!(!inline_card("mcp__crundi__widget_open", &json!({ "slot": "cell", "id": "x" })));
    }
}
