//! Panels (widgets: the tab, the widget pane, the inline card, the dock,
//! chips; Slint: src/ops/panels.rs, ui/ops/panels*.slint, specs/ops/panels.md).
//!
//! A panel ("widget") is HTML Claude wrote. The web puts it in a sandboxed
//! iframe and talks to it with postMessage (app/vendor/crundi-widgets.js);
//! here every copy on screen is an off-screen CEF page (crundi-webview's
//! engine panels, drawn by its `PageView`), by frame key
//! "wg|<frame>|<project>|<id>" (frame: tab, cell, inline:<chat>, dock:<chat>,
//! chip, pop). A copy is made when it is first drawn; its document
//! (/api/widgets/doc) and data (/api/widgets/data) are fetched and handed to
//! the page as a data: URL; a copy not drawn for a few seconds gives its
//! browser back, and loads again when it is drawn again.
//!
//! The bridge, as the Slint client's: in, the page is built with its first
//! message (`init`: data, store, context) inside it, answered when the
//! runtime says `ready`; a later change (new data, a store saved elsewhere,
//! a new version) loads the page again with the new values. Out, a small
//! script ahead of the runtime writes the runtime's messages (ready, height,
//! fault, call) into the page's title, read here every tick.
//!
//! Everything else is the web's: the list, placement, the tab's bar and
//! panes, grants, the chooser, docks, chips and their popover, events.
//! view.rs draws it.

mod view;
pub use view::{chips, dock, inline_card, overlay};

use crundi_core::api::ApiError;
use crundi_shell::AreaEvent;
use crundi_webview::{engine, PageView};
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc, time::{Duration, Instant}};

// ─── Small readers (the Slint kit's) ───

pub(crate) fn s(v: &Value, k: &str) -> String {
    match v.get(k) { None | Some(Value::Null) => String::new(), Some(Value::String(x)) => x.clone(), Some(o) => o.to_string() }
}
pub(crate) fn n(v: &Value, k: &str) -> f64 {
    match v.get(k) { Some(Value::Number(x)) => x.as_f64().unwrap_or(0.0), Some(Value::String(x)) => x.parse().unwrap_or(0.0), _ => 0.0 }
}
pub(crate) fn truthy(v: &Value) -> bool {
    match v { Value::Null => false, Value::Bool(b) => *b, Value::Number(x) => x.as_f64().unwrap_or(0.0) != 0.0, Value::String(x) => !x.is_empty(), _ => true }
}
fn flag(v: &Value, k: &str) -> bool { v.get(k).is_some_and(truthy) }
/// A query value, percent-encoded.
pub(crate) fn enc(t: &str) -> String {
    t.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// A toast, after the current update (the window is free then).
pub(crate) fn toast(cx: &mut App, text: impl Into<String>, kind: w::Toast) {
    let t: String = text.into();
    with_window(cx, move |window, cx| w::toast(window, cx, t, kind));
}
pub(crate) fn toast_error(cx: &mut App, text: impl Into<String>) { toast(cx, text, w::Toast::Error) }

/// Run `f` with the window, after the current update.
pub(crate) fn with_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    cx.defer(move |cx| {
        let Some(h) = cx.active_window().or_else(|| cx.windows().first().copied()) else { return };
        let _ = h.update(cx, move |_, window, cx| f(window, cx));
    });
}

// ─── Pure rules (unit-tested) ───

/// The web's SLOT_LABEL.
pub const SLOT_LABEL: [(&str, &str); 5] = [("dock", "Docked to chat"), ("cell", "Workbench pane"), ("tab", "Panels tab only"), ("inline", "Inline in chat"), ("chip", "Top bar chip")];
/// Where a person may move a panel to.
pub const MOVABLE: [&str; 4] = ["dock", "cell", "tab", "chip"];

pub fn slot_label(slot: &str) -> &'static str {
    SLOT_LABEL.iter().find(|(k, _)| *k == slot).map(|(_, l)| *l).unwrap_or("Elsewhere")
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
    if s(w, "state") != "open" { return "none"; }
    let chat = s(w, "chat");
    match s(w, "slot").as_str() {
        "dock" if chat.is_empty() => "cell",
        "dock" => if live_chats.contains(&chat) { "dock" } else { "none" },
        "cell" => "cell",
        "chip" => "chip",
        _ => "none",
    }
}

/// The slot <select>: labels, values and the current one. Dock only for a
/// panel with a chat; a slot that is not movable (inline) is listed first.
pub fn slot_options(w: &Value) -> (Vec<&'static str>, Vec<String>, usize) {
    let slot = s(w, "slot");
    let chat = s(w, "chat");
    let mut values: Vec<String> = MOVABLE.iter().filter(|x| !(**x == "dock" && chat.is_empty())).map(|x| x.to_string()).collect();
    if !MOVABLE.contains(&slot.as_str()) { values.insert(0, slot.clone()); }
    let labels = values.iter().map(|v| slot_label(v)).collect();
    let i = values.iter().position(|v| *v == slot).unwrap_or(usize::MAX);
    (labels, values, i)
}

/// A tab row's line: "closed", or its slot's first word ("docked",
/// "workbench", "panels", "inline", "top").
pub fn item_sub(w: &Value) -> String {
    if s(w, "state") != "open" { return "closed".into(); }
    slot_label(&s(w, "slot")).split(' ').next().unwrap_or("").to_lowercase()
}

/// A tab row's badge and its tone: waiting for approval, or faulted.
pub fn item_badge(w: &Value) -> (&'static str, &'static str) {
    if flag(w, "needsGrant") { ("approve", "warn") } else if n(w, "faults") > 0.0 { ("error", "error") } else { ("", "") }
}

/// The tab's order: open first, then the newest.
pub fn tab_order(list: &[Value]) -> Vec<Value> {
    let mut v = list.to_vec();
    v.sort_by(|a, b| {
        let oa = (s(a, "state") != "open") as u8;
        let ob = (s(b, "state") != "open") as u8;
        oa.cmp(&ob).then(n(b, "updatedAt").total_cmp(&n(a, "updatedAt")))
    });
    v
}

/// The chooser's line for a panel.
pub fn pick_status(w: &Value, place: &str) -> &'static str {
    if s(w, "state") != "open" { "closed" } else { match place { "cell" => "already open", "dock" => "docked to a chat", "chip" => "top bar", _ => "Panels tab" } }
}

/// The tab bar's scope control: "local" (Make global), "global" (Global:
/// back to this project), "other" (a badge: from another project).
pub fn scope_of(w: &Value, project: &str) -> &'static str {
    if !flag(w, "global") { return "local"; }
    let home = s(w, "home");
    if home.is_empty() || home == pkey(project) { "global" } else { "other" }
}

/// A dotted path into a value (a chip's value).
pub fn path_get<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() { return Some(v); }
    path.split('.').try_fold(v, |cur, p| cur.get(p))
}

/// A chip's value as text: at most 60 characters, objects as JSON.
pub fn chip_text(v: Option<&Value>) -> String {
    let t = match v { None | Some(Value::Null) => String::new(), Some(Value::String(x)) => x.clone(), Some(o) => o.to_string() };
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

pub fn frame_key(frame: &str, project: &str, id: &str) -> String { format!("wg|{frame}|{project}|{id}") }

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
    let (nn, body) = rest.split_once(' ')?;
    if nn.parse::<u64>().ok()? != nonce { return None; }
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

/// A picture's size from its header (PNG; anything else 16:10).
pub fn image_size(b: &[u8]) -> (f32, f32) {
    if b.len() >= 24 && &b[1..4] == b"PNG" {
        let w = u32::from_be_bytes([b[16], b[17], b[18], b[19]]) as f32;
        let h = u32::from_be_bytes([b[20], b[21], b[22], b[23]]) as f32;
        if w > 0.0 && h > 0.0 { return (w, h); }
    }
    (16.0, 10.0)
}

/// Does a tool call show an inline card under it (widget_open, slot inline)?
pub fn is_inline_card(tool: &str, input: &Value) -> bool { tool.ends_with("widget_open") && s(input, "slot") == "inline" && !s(input, "id").is_empty() }

// ─── State ───

pub(crate) struct Grant { hash: String, pub(crate) items: Vec<(String, String)> }

#[derive(Default)]
pub(crate) struct View {
    frame: String,
    alias: String,
    id: String,
    /// Each load of the page; the title channel names it.
    nonce: u64,
    loading: bool,
    /// "" not yet, "loading", "error", "ready".
    pub(crate) state: String,
    pub(crate) error: String,
    pub(crate) grant: Option<Grant>,
    pub(crate) grant_hidden: bool,
    pub(crate) approving: bool,
    pub(crate) height: f32,
    pub(crate) measured: bool,
    chip_w: f32,
    pub(crate) title: String,
    last_seq: u64,
    faults: u32,
    calls: Vec<Instant>,
    store_call: Option<Instant>,
    size: (f32, f32),
    seen: Option<Instant>,
    /// Not drawn: its page hidden in the browser host (no frames) until drawn again.
    hidden: bool,
    handed: Option<Instant>,
    reload_at: Option<Instant>,
    msgs: u64,
    /// The engine's panel showing it, and its view.
    panel: Option<usize>,
    pub(crate) page: Option<Entity<PageView>>,
}

#[derive(Default)]
struct St {
    project: String,
    widgets: Vec<Value>,
    loaded: bool,
    error: String,
    seq: u64,
    selected: String,
    pane: String,
    versions: Vec<Value>,
    versions_state: String,
    shots: Vec<(String, Option<(Arc<Image>, f32, f32)>)>,
    checks_state: String,
    checks_seq: u64,
    /// A chooser pane's panel: its key -> (the panel, when picked).
    picked: HashMap<String, (String, Instant)>,
    chip_values: HashMap<String, String>,
    pop: String,
    views: HashMap<String, View>,
    cells_sig: String,
    opened: String,
    last_call: Value,
    ticks: u64,
    /// The driver's look at an inline card: (project, id, chat).
    preview: (String, String, String),
    started: bool,
}

thread_local! { static ST: RefCell<St> = RefCell::new(St::default()); }

fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { ST.with(|x| f(&mut x.borrow_mut())) }

fn bump(cx: &mut App) { cx.refresh_windows(); }

fn by_id(st: &St, id: &str) -> Option<Value> { st.widgets.iter().find(|w| s(w, "id") == id).cloned() }

/// The chats of the project on screen (a docked panel needs its chat).
fn live_chats(project: &str, cx: &App) -> Vec<String> {
    crundi_shell::session::get(cx).terminals.iter()
        .filter(|t| s(t, "kind") == "ui" && s(t, "project").eq_ignore_ascii_case(project))
        .map(|t| s(t, "id")).collect()
}

fn ok_of(v: &Value) -> bool { flag(v, "ok") }
fn err_or(v: &Value, d: &str) -> String { Some(s(v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| d.to_string()) }

/// The prefix of a workbench pane showing a panel placed in the workbench
/// ("panel:widget:@<id>"); "panel:widget:<n>" is a chooser (the "+" menu's Panel).
const AUTO: &str = "panel:widget:@";

// ─── Public: for the chat side, the workbench, the top bar ───

/// The ids of the panels that are workbench panes now.
pub fn cell_ids(cx: &App) -> Vec<String> {
    let (list, p) = with(|st| (st.widgets.clone(), st.project.clone()));
    if p.is_empty() { return vec![]; }
    let live = live_chats(&p, cx);
    list.iter().filter(|w| place_of(w, &live) == "cell").map(|w| s(w, "id")).collect()
}

/// A panel's title (a pane's header).
pub fn title_of(id: &str) -> String { with(|st| by_id(st, id).map(|w| s(&w, "title")).unwrap_or_default()) }

/// Is a panel waiting for the owner's approval (a pane header's "approve")?
pub fn needs_grant(id: &str) -> bool { with(|st| by_id(st, id).is_some_and(|w| flag(&w, "needsGrant"))) }

/// A pane header's Reload and × (the web's widget-reload, widget-close).
pub fn reload(id: &str, cx: &mut App) { reload_id(id, cx); }
pub fn close(id: &str, cx: &mut App) { user_patch(id, json!({ "closed": true }), cx, |_, _| {}); }

/// A pane header's slot control: the slots as a menu at a window point.
pub fn slot_menu(id: &str, at: Point<Pixels>, cx: &mut App) {
    let Some(wv) = with(|st| by_id(st, id)) else { return };
    let (labels, values, cur) = slot_options(&wv);
    let items = labels.iter().zip(values.iter()).enumerate()
        .map(|(i, (l, v))| { let e = w::MenuEntry::new(v, *l); if i == cur { e.icon(w::IconName::Check) } else { e } }).collect();
    let id = id.to_string();
    w::menu_at(cx, at, items, move |v, _, cx| { user_patch(&id, json!({ "slot": v }), cx, |_, _| {}); });
}

// ─── The list ───

fn refresh(cx: &mut App) {
    let (p, seq) = with(|st| { st.seq += 1; (st.project.clone(), st.seq) });
    if crundi_shell::api(cx).is_none() { return; }
    crundi_shell::call(cx, move |api| api.get(&format!("/api/widgets?project={}", enc(&p))), move |r, cx| {
        if with(|st| st.seq != seq) { return; }
        with(|st| match r {
            Ok(v) => {
                st.widgets = if ok_of(&v) { v.get("widgets").and_then(|x| x.as_array()).cloned().unwrap_or_default() } else { vec![] };
                st.loaded = true;
                st.error.clear();
            }
            // Shown (with Retry) unless panels are on screen already.
            Err(e) => st.error = e,
        });
        after_list(cx);
    });
}

/// The list changed: the selection, the choosers' picks, chips, panes.
fn after_list(cx: &mut App) {
    let (p, list) = with(|st| (st.project.clone(), st.widgets.clone()));
    let live = live_chats(&p, cx);
    with(|st| {
        let order = tab_order(&st.widgets);
        if by_id(st, &st.selected).is_none() { st.selected = order.first().map(|w| s(w, "id")).unwrap_or_default(); st.pane.clear(); }
        // A chooser whose panel left the workbench shows the list again
        // (a pick just made waits for the server's answer).
        let gone: Vec<String> = st.picked.iter()
            .filter(|(_, (id, at))| at.elapsed() > Duration::from_secs(5) && by_id(st, id).is_none_or(|w| place_of(&w, &live) != "cell"))
            .map(|(k, _)| k.clone()).collect();
        for k in gone { st.picked.remove(&k); }
        if !st.pop.is_empty() && by_id(st, &st.pop).is_none() { st.pop.clear(); }
    });
    for wv in list.iter().filter(|w| place_of(w, &live) == "chip") { load_chip_value(wv, cx); }
    sync_cells(cx);
    bump(cx);
}

/// The panels placed in the workbench are panes there (the web's widget
/// cells): one "panel:widget:@<id>" pane each, unless a chooser shows it;
/// a pane whose panel left goes. Only when the set changes, so a pane
/// closed by hand stays closed.
fn sync_cells(cx: &mut App) {
    let p = with(|st| st.project.clone());
    if p.is_empty() { return; }
    let cells = cell_ids(cx);
    let sig = format!("{p}|{}", cells.join(","));
    if with(|st| { let same = st.cells_sig == sig; st.cells_sig = sig; same }) { return; }
    let Some(wb) = crundi_workbench::workbench(cx) else { return };
    let picked: Vec<String> = with(|st| st.picked.values().map(|x| x.0.clone()).collect());
    wb.update(cx, |wb, cx| {
        if !wb.project.eq_ignore_ascii_case(&p) { return; }
        let keys = wb.mosaic().keys();
        let stale: Vec<String> = keys.iter().filter(|k| k.strip_prefix(AUTO).is_some_and(|id| !cells.iter().any(|c| c == id))).cloned().collect();
        if !stale.is_empty() { wb.retain(move |k| !stale.iter().any(|x| x == k), cx); }
        for id in &cells {
            let k = format!("{AUTO}{id}");
            if !keys.contains(&k) && !picked.contains(id) { wb.place(&k, cx); }
        }
    });
}

fn load_chip_value(wv: &Value, cx: &mut App) {
    let Some(chip) = wv.get("chip").filter(|c| c.is_object()) else { return };
    let source = s(chip, "source");
    if source.is_empty() { return; }
    let path = s(chip, "path");
    let id = s(wv, "id");
    let p = with(|st| st.project.clone());
    let q = format!("/api/widgets/data?project={}&id={}&source={}", enc(&p), enc(&id), enc(&source));
    crundi_shell::call(cx, move |api| api.get(&q), move |r, cx| {
        let Ok(d) = r else { return };
        let entry = if ok_of(&d) { d.get("data").and_then(|x| x.get(&source)).and_then(|e| e.get("value")) } else { None };
        let text = chip_text(entry.and_then(|v| path_get(v, &path)));
        with(|st| st.chip_values.insert(id, text));
        bump(cx);
    });
}

/// A version's time as the web's toLocaleString.
fn when(ms: f64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(ms as i64).single().map(|t| t.format("%d/%m/%Y, %H:%M:%S").to_string()).unwrap_or_default()
}

// ─── The tab ───

fn select(id: &str, cx: &mut App) {
    with(|st| { st.selected = id.to_string(); st.pane.clear(); });
    bump(cx);
}

fn show_pane(pane: &str, cx: &mut App) {
    let (p, id) = with(|st| { st.pane = pane.to_string(); (st.project.clone(), st.selected.clone()) });
    if pane == "history" {
        with(|st| { st.versions.clear(); st.versions_state = "loading".into(); });
        let q = format!("/api/widgets/versions?project={}&id={}", enc(&p), enc(&id));
        crundi_shell::call(cx, move |api| api.get(&q), move |r, cx| {
            let list = r.ok().filter(ok_of).and_then(|d| d.get("versions").and_then(|x| x.as_array()).cloned()).unwrap_or_default();
            with(|st| { st.versions_state = if list.is_empty() { "empty".into() } else { "list".into() }; st.versions = list; });
            bump(cx);
        });
    }
    if pane == "checks" { load_shots(cx); }
    bump(cx);
}

/// The screenshots of Claude's last check (the web's shotsNode).
fn load_shots(cx: &mut App) {
    let (p, id, seq) = with(|st| { st.checks_seq += 1; st.shots.clear(); st.checks_state = "loading".into(); (st.project.clone(), st.selected.clone(), st.checks_seq) });
    let q = format!("/api/widgets/renders?project={}&id={}", enc(&p), enc(&id));
    crundi_shell::call(cx, move |api| api.get(&q), move |r, cx| {
        if with(|st| st.checks_seq != seq) { return; }
        let renders = r.ok().filter(ok_of).and_then(|d| d.get("renders").cloned()).unwrap_or(Value::Null);
        let shots = renders.get("shots").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let at = s(&renders, "at");
        with(|st| {
            st.checks_state = if shots.is_empty() { "none".into() } else { "list".into() };
            st.shots = shots.iter().map(|sh| {
                let (f, stt) = (s(sh, "frame"), s(sh, "state"));
                (if stt != "live" { format!("{f} · {stt}") } else { f }, None)
            }).collect();
        });
        bump(cx);
        let Some(api) = crundi_shell::api(cx) else { return };
        for (i, sh) in shots.iter().enumerate() {
            let name = format!("{}--{}", s(sh, "frame"), s(sh, "state"));
            let path = format!("/api/widgets/render-image?project={}&id={}&name={}&t={}", enc(&p), enc(&id), enc(&name), enc(&at));
            let api = api.clone();
            crundi_shell::run(cx, move || api.get_bytes(&path).ok().filter(|(st, b)| *st == 200 && !b.is_empty()).map(|(_, b)| b), move |b, cx| {
                let Some(b) = b else { return };
                if with(|st| st.checks_seq != seq) { return; }
                let (w0, h0) = image_size(&b);
                let fmt = if b.starts_with(b"\x89PNG") { ImageFormat::Png } else if b.starts_with(b"RIFF") { ImageFormat::Webp } else { ImageFormat::Jpeg };
                let img = Arc::new(Image::from_bytes(fmt, b));
                with(|st| if let Some(x) = st.shots.get_mut(i) { x.1 = Some((img, w0, h0)); });
                bump(cx);
            });
        }
    });
}

fn user_patch(id: &str, patch: Value, cx: &mut App, then: impl FnOnce(bool, &mut App) + 'static) {
    let p = with(|st| st.project.clone());
    let body = json!({ "project": p, "id": id, "patch": patch });
    crundi_shell::call(cx, move |api| api.post("/api/widgets/user", &body), move |r, cx| {
        let ok = match r {
            Ok(v) if ok_of(&v) => true,
            Ok(v) => { toast_error(cx, err_or(&v, "Could not update the panel")); false }
            Err(e) => { toast_error(cx, e); false }
        };
        then(ok, cx);
    });
}

fn set_scope(id: &str, global: bool, cx: &mut App) {
    let p = with(|st| st.project.clone());
    let body = json!({ "project": p, "id": id, "global": global });
    crundi_shell::call(cx, move |api| api.post("/api/widgets/scope", &body), move |r, cx| {
        let v = match r { Ok(v) => v, Err(e) => return toast_error(cx, e) };
        if !ok_of(&v) { return toast_error(cx, err_or(&v, if global { "Could not make it global" } else { "Could not change it" })); }
        if global {
            let k = v.get("inPlace").and_then(|x| x.as_array()).map(|a| a.len()).unwrap_or(0);
            toast(cx, format!("Now global: shown in every project{}", if k > 0 { format!(". It still reads {k} file{} from this project", if k == 1 { "" } else { "s" }) } else { String::new() }), w::Toast::Info);
        } else {
            toast(cx, "Back in this project only", w::Toast::Info);
        }
    });
}

fn delete(id: &str, window: &mut Window, cx: &mut App) {
    let Some(wv) = with(|st| by_id(st, id)) else { return };
    let (p, id) = (with(|st| st.project.clone()), id.to_string());
    let msg = format!("Delete \"{}\" and its source files? This cannot be undone.", s(&wv, "title"));
    w::ask_confirm(window, cx, w::Ask { title: "Delete panel".into(), message: msg.into(), ok: "Delete".into(), danger: true, ..Default::default() }, move |ok, _, cx| {
        if !ok { return; }
        let body = json!({ "project": p, "id": id });
        crundi_shell::call(cx, move |api| api.post("/api/widgets/delete", &body), |r, cx| match r {
            Ok(v) if !ok_of(&v) => toast_error(cx, err_or(&v, "Could not delete")),
            Err(e) => toast_error(cx, e),
            _ => {}
        });
    });
}

fn restore(id: &str, version: &str, cx: &mut App) {
    let p = with(|st| st.project.clone());
    let ver: Value = version.parse::<u64>().map(Value::from).unwrap_or_else(|_| Value::from(version));
    let body = json!({ "project": p, "id": id, "version": ver });
    let version = version.to_string();
    crundi_shell::call(cx, move |api| api.post("/api/widgets/rollback", &body), move |r, cx| match r {
        Ok(v) if ok_of(&v) => { toast(cx, format!("Restored v{version}"), w::Toast::Info); show_pane("", cx); }
        Ok(v) => toast_error(cx, err_or(&v, "Could not restore")),
        Err(e) => toast_error(cx, e),
    });
}

/// Reload: every copy of the panel in this project loads again.
fn reload_id(id: &str, cx: &mut App) {
    let pk = with(|st| pkey(&st.project));
    let keys: Vec<String> = with(|st| st.views.iter().filter(|(_, v)| v.id == id && pkey(&v.alias) == pk).map(|(k, _)| k.clone()).collect());
    for k in keys { load(&k, cx); }
}

fn slot(id: &str, v: &str, cx: &mut App) {
    let in_pop = with(|st| st.pop == id);
    user_patch(id, json!({ "slot": v }), cx, move |_, cx| if in_pop { close_pop(cx); });
}

fn pin(id: &str, cx: &mut App) {
    let pinned = with(|st| by_id(st, id).is_some_and(|w| s(&w, "lifecycle") == "pinned"));
    user_patch(id, json!({ "pinned": !pinned }), cx, |_, _| {});
}

fn reopen(id: &str, cx: &mut App) { user_patch(id, json!({ "closed": false }), cx, |_, _| {}); }

fn retry(cx: &mut App) { with(|st| st.error.clear()); bump(cx); refresh(cx); }

fn open_tab(id: &str, cx: &mut App) {
    with(|st| { st.selected = id.to_string(); st.pane.clear(); });
    if let Some(sh) = crundi_shell::shell(cx) { sh.update(cx, |sh, cx| sh.show_tab("panels", cx)); }
    bump(cx);
}

// ─── Views (the copies on screen) ───

/// A copy's view, made the first time it is drawn (its first load then).
pub(crate) fn ensure_view(key: &str, cx: &mut App) -> Option<Entity<PageView>> {
    let (frame, alias, id) = parse_key(key)?;
    if let Some(p) = with(|st| st.views.get(key).and_then(|v| v.page.clone())) { return Some(p); }
    let page = cx.new(PageView::new);
    let first = with(|st| {
        let v = st.views.entry(key.to_string()).or_insert_with(|| View { frame, alias, id, ..Default::default() });
        v.page = Some(page.clone());
        v.seen = Some(Instant::now());
        v.state.is_empty()
    });
    if first { let k = key.to_string(); cx.defer(move |cx| load(&k, cx)); }
    Some(page)
}

/// A copy's box was laid out: it is on screen (and this big). One that gave
/// its browser back loads again.
pub(crate) fn seen_box(key: &str, b: Bounds<Pixels>, cx: &mut App) {
    let (w0, h0) = (f32::from(b.size.width), f32::from(b.size.height));
    let again = with(|st| {
        let Some(v) = st.views.get_mut(key) else { return false };
        v.seen = Some(Instant::now());
        v.size = (w0, h0);
        // Drawn again: shown in the host at once.
        if std::mem::take(&mut v.hidden) { if let Some(i) = v.panel { engine::set_parked(i, false); } }
        v.state == "ready" && !v.loading && v.panel.is_none() && v.reload_at.is_none() && v.handed.is_none_or(|t| t.elapsed() > Duration::from_millis(1500))
    });
    if again { with(|st| if let Some(v) = st.views.get_mut(key) { v.reload_at = Some(Instant::now()); }); let _ = cx; }
}

/// The engine panel is there and has drawn.
fn panel_live(i: Option<usize>) -> (bool, bool) {
    let Some(i) = i else { return (false, false) };
    engine::with(|st| st.panels.get(i).map(|p| (!p.closed && p.browser.is_some(), !p.closed && p.frames > 0)).unwrap_or((false, false)))
}

pub(crate) fn painted(key: &str) -> bool { panel_live(with(|st| st.views.get(key).and_then(|v| v.panel))).1 }

/// Load (or reload) a view's page: its document and data, then the page.
fn load(key: &str, cx: &mut App) {
    let Some((alias, id, frame, nonce)) = with(|st| st.views.get_mut(key).map(|v| {
        v.loading = true;
        v.nonce += 1;
        v.reload_at = None;
        if v.state.is_empty() { v.state = "loading".into(); }
        (v.alias.clone(), v.id.clone(), v.frame.clone(), v.nonce)
    })) else { return };
    bump(cx);
    let chip = frame == "chip";
    let doc_q = format!("/api/widgets/doc?project={}&id={}{}", enc(&alias), enc(&id), if chip { "&chip=1" } else { "" });
    let data_q = format!("/api/widgets/data?project={}&id={}", enc(&alias), enc(&id));
    let key = key.to_string();
    crundi_shell::call(cx, move |api| {
        let doc = match api.get(&doc_q) {
            Ok(d) => d,
            // "No such widget" comes as a 404 with the server's words.
            Err(ApiError::Status(404, m)) if !m.is_empty() => json!({ "ok": false, "error": m }),
            Err(e) => return Err(e),
        };
        let data = if ok_of(&doc) { api.get(&data_q).unwrap_or(Value::Null) } else { Value::Null };
        Ok(json!({ "doc": doc, "data": data }))
    }, move |r, cx| loaded(&key, nonce, r, cx));
}

fn loaded(key: &str, nonce: u64, r: Result<Value, String>, cx: &mut App) {
    let mut hand: Option<String> = None;
    let stale = with(|st| {
        let Some(v) = st.views.get_mut(key) else { return true };
        if v.nonce != nonce { return true; }
        v.loading = false;
        match &r {
            Err(e) => { v.state = "error".into(); v.error = e.clone(); }
            Ok(d) => {
                let doc = d.get("doc").cloned().unwrap_or(Value::Null);
                if !ok_of(&doc) {
                    v.state = "error".into();
                    v.error = err_or(&doc, "This panel could not be loaded.");
                } else if !crundi_webview::started() {
                    v.state = "error".into();
                    v.error = "The browser did not start, so panels cannot be shown here.".into();
                } else {
                    v.state = "ready".into();
                    v.error.clear();
                    v.title = doc.get("widget").map(|x| s(x, "title")).unwrap_or_default();
                    let g = doc.get("grant").cloned().unwrap_or(Value::Null);
                    v.grant = (flag(&g, "needed") && !flag(&g, "granted")).then(|| Grant {
                        hash: s(&g, "hash"),
                        items: g.get("items").and_then(|x| x.as_array()).map(|a| a.iter().map(|i| (s(i, "what"), s(i, "detail"))).collect()).unwrap_or_default(),
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
                    hand = Some(build_html(&s(&doc, "html"), nonce, &init));
                    v.handed = Some(Instant::now());
                }
            }
        }
        false
    });
    if stale { return; }
    if let Some(html) = hand { show_html(key, &html, cx); }
    bump(cx);
}

/// Put the page in the view's engine panel (made, or navigated again).
fn show_html(key: &str, html: &str, cx: &mut App) {
    use base64::Engine as _;
    let url = format!("data:text/html;charset=utf-8;base64,{}", base64::engine::general_purpose::STANDARD.encode(html.as_bytes()));
    let Some((panel, page, size)) = with(|st| st.views.get(key).map(|v| (v.panel, v.page.clone(), v.size))) else { return };
    let i = match panel {
        Some(i) if panel_live(Some(i)).0 => { engine::navigate(i, &url, Box::new(|| {})); i }
        other => {
            if let Some(i) = other { engine::close_panel(i); }
            let i = engine::open_panel(&url, "client");
            if size.0 > 1.0 && size.1 > 1.0 { engine::resized(i, size.0, size.1); }
            i
        }
    };
    with(|st| if let Some(v) = st.views.get_mut(key) { v.panel = Some(i); });
    if let Some(p) = page { p.update(cx, |p, cx| p.set_panel(Some(i), cx)); }
}

/// A view gives its browser back (not drawn for a while, or signed out).
fn release(key: &str, cx: &mut App) {
    let (panel, page) = with(|st| st.views.get_mut(key).map(|v| { v.handed = None; v.hidden = false; (v.panel.take(), v.page.clone()) }).unwrap_or((None, None)));
    if let Some(i) = panel { engine::close_panel(i); }
    if let Some(p) = page { p.update(cx, |p, cx| p.set_panel(None, cx)); }
}

/// The page's messages, from its title (see the top of this file).
fn read_title(key: &str, cx: &mut App) {
    let Some((nonce, panel)) = with(|st| st.views.get(key).map(|v| (v.nonce, v.panel))) else { return };
    let Some(i) = panel else { return };
    let title = engine::with(|st| st.panels.get(i).map(|p| p.title.clone()).unwrap_or_default());
    let Some(msgs) = parse_title(&title, nonce) else { return };
    let fresh: Vec<Value> = with(|st| {
        let Some(v) = st.views.get_mut(key) else { return vec![] };
        let out: Vec<Value> = msgs.iter().filter(|(q, _)| *q > v.last_seq).map(|(_, m)| m.clone()).collect();
        if let Some((q, _)) = msgs.last() { v.last_seq = v.last_seq.max(*q); }
        v.msgs += out.len() as u64;
        out
    });
    for m in fresh { on_message(key, &m, cx); }
}

fn on_message(key: &str, m: &Value, cx: &mut App) {
    match s(m, "t").as_str() {
        "height" => {
            with(|st| if let Some(v) = st.views.get_mut(key) {
                v.height = n(m, "h") as f32;
                v.measured = true;
                if m.get("w").is_some() { v.chip_w = n(m, "w") as f32; }
            });
            bump(cx);
        }
        "fault" => {
            let Some((alias, id, k)) = with(|st| st.views.get_mut(key).map(|v| { v.faults += 1; (v.alias.clone(), v.id.clone(), v.faults) })) else { return };
            // A handful is a diagnosis; a flood is a loop.
            if k > 5 { return; }
            let body = json!({ "project": alias, "id": id, "message": s(m, "message").chars().take(500).collect::<String>(), "where": s(m, "where").chars().take(120).collect::<String>() });
            crundi_shell::call(cx, move |api| api.post("/api/widgets/fault", &body), |_, _| {});
        }
        "call" => on_call(key, m, cx),
        _ => {}
    }
}

/// Something done inside a panel: toast and openLink here, the rest by the server.
fn on_call(key: &str, m: &Value, cx: &mut App) {
    let Some((alias, id, ok)) = with(|st| st.views.get_mut(key).map(|v| {
        let now = Instant::now();
        v.calls.retain(|t| now.duration_since(*t) < Duration::from_secs(10));
        let ok = v.calls.len() < 40;
        if ok { v.calls.push(now); }
        (v.alias.clone(), v.id.clone(), ok)
    })) else { return };
    if !ok { return; }
    let op = s(m, "op");
    let a = m.get("args").cloned().unwrap_or(Value::Null);
    match op.as_str() {
        "toast" => {
            let text: String = s(&a, "text").chars().take(200).collect();
            toast(cx, text, if s(&a, "kind") == "error" { w::Toast::Error } else { w::Toast::Info });
            with(|st| st.last_call = json!({ "op": op, "ok": true }));
        }
        "openLink" => {
            let url = s(&a, "url");
            let lower = url.to_lowercase();
            // http(s) only: never javascript: or data: in a real browser.
            if lower.starts_with("http://") || lower.starts_with("https://") {
                with(|st| st.opened = url.clone());
                if std::env::var_os("CRUNDI_TEST_PORT").is_none() { let _ = open::that(&url); }
            }
            with(|st| st.last_call = json!({ "op": op, "url": url }));
        }
        _ => {
            if op == "store.set" { with(|st| if let Some(v) = st.views.get_mut(key) { v.store_call = Some(Instant::now()); }); }
            let body = json!({ "project": alias, "id": id, "op": op, "args": a });
            let op2 = op.clone();
            crundi_shell::call(cx, move |api| api.post("/api/widgets/call", &body), move |r, _| {
                let res = match r { Ok(v) => v, Err(e) => json!({ "ok": false, "error": e }) };
                with(|st| st.last_call = json!({ "op": op2, "result": res }));
            });
        }
    }
}

fn approve(key: &str, cx: &mut App) {
    let Some((alias, id, hash)) = with(|st| st.views.get_mut(key).and_then(|v| { v.approving = true; v.grant.as_ref().map(|g| (v.alias.clone(), v.id.clone(), g.hash.clone())) })) else { return };
    bump(cx);
    let body = json!({ "project": alias, "id": id, "hash": hash });
    let key = key.to_string();
    crundi_shell::call(cx, move |api| api.post("/api/widgets/grant", &body), move |r, cx| {
        let v = r.unwrap_or_else(|e| json!({ "ok": false, "error": e }));
        if ok_of(&v) { return toast(cx, "Approved", w::Toast::Info); }
        toast_error(cx, err_or(&v, "Could not approve"));
        if flag(&v, "stale") { load(&key, cx); } else { with(|st| if let Some(v) = st.views.get_mut(&key) { v.approving = false; }); bump(cx); }
    });
}

fn not_now(key: &str, cx: &mut App) {
    with(|st| if let Some(v) = st.views.get_mut(key) { v.grant_hidden = true; });
    bump(cx);
}

/// What a copy shows: (state, error, grant items when the banner shows,
/// approving, height when measured, title, chip width).
pub(crate) struct FrameInfo { pub state: String, pub error: String, pub grant: Option<Vec<(String, String)>>, pub approving: bool, pub height: Option<f32>, pub title: String }

pub(crate) fn frame_info(key: &str) -> FrameInfo {
    with(|st| match st.views.get(key) {
        None => FrameInfo { state: "loading".into(), error: String::new(), grant: None, approving: false, height: None, title: String::new() },
        Some(v) => FrameInfo {
            state: if v.state.is_empty() { "loading".into() } else { v.state.clone() }, error: v.error.clone(),
            grant: v.grant.as_ref().filter(|_| !v.grant_hidden).map(|g| g.items.clone()), approving: v.approving,
            height: v.measured.then_some(v.height), title: v.title.clone(),
        },
    })
}

/// The panels docked to a chat: (id, title, approve, collapsed, the slot select).
pub(crate) fn docks(chat: &str, cx: &App) -> Vec<Value> {
    let (p, list) = with(|st| (st.project.clone(), st.widgets.clone()));
    let mut live = live_chats(&p, cx);
    // The chat asking is on screen, whatever the last state said.
    if !live.iter().any(|c| c == chat) { live.push(chat.to_string()); }
    list.into_iter().filter(|w| s(w, "chat") == chat && place_of(w, &live) == "dock").collect()
}

/// A pane's panel: "panel:widget:@<id>" that one; a chooser's pick ("" none yet).
pub(crate) fn picked_of(key: &str) -> String {
    if let Some(id) = key.strip_prefix(AUTO) { return id.to_string(); }
    with(|st| st.picked.get(key).map(|x| x.0.clone()).unwrap_or_default())
}

fn pick(key: &str, id: &str, cx: &mut App) {
    with(|st| st.picked.insert(key.to_string(), (id.to_string(), Instant::now())));
    bump(cx);
    let (k, i) = (key.to_string(), id.to_string());
    user_patch(id, json!({ "closed": false, "slot": "cell" }), cx, move |ok, cx| {
        if !ok { with(|st| if st.picked.get(&k).is_some_and(|x| x.0 == i) { st.picked.remove(&k); }); bump(cx); }
    });
}

fn collapse(id: &str, c: bool, cx: &mut App) {
    with(|st| if let Some(w) = st.widgets.iter_mut().find(|w| s(w, "id") == id) { w["collapsed"] = Value::Bool(c); });
    bump(cx);
    user_patch(id, json!({ "collapsed": c }), cx, |_, _| {});
}

fn open_pop(id: &str, cx: &mut App) { with(|st| st.pop = id.to_string()); bump(cx); }
fn close_pop(cx: &mut App) { with(|st| st.pop.clear()); bump(cx); }

// ─── Live events ───

fn on_widget_event(d: &Value, cx: &mut App) {
    let project = s(d, "project");
    let g = project == "_global";
    let mine = with(|st| pkey(&st.project));
    if !g && project != mine { return; }
    let id = s(d, "id");
    let kind = s(d, "kind");
    let matching = |skip_store: bool| -> Vec<String> {
        with(|st| st.views.iter().filter(|(_, v)| v.id == id && (g || pkey(&v.alias) == project))
            .filter(|(_, v)| !(skip_store && v.store_call.is_some_and(|t| t.elapsed() < Duration::from_secs(3))))
            .map(|(k, _)| k.clone()).collect())
    };
    match kind.as_str() {
        "data" | "store" => {
            // New values: the page loads again with them.
            let keys = matching(kind == "store");
            with(|st| for k in &keys { if let Some(v) = st.views.get_mut(k) { v.reload_at.get_or_insert(Instant::now() + Duration::from_millis(300)); } });
            if kind == "data" {
                let source = s(d, "source");
                let wv = with(|st| by_id(st, &id));
                let live = live_chats(&with(|st| st.project.clone()), cx);
                if let Some(wv) = wv.filter(|w| place_of(w, &live) == "chip") {
                    let cs = wv.get("chip").map(|c| s(c, "source")).unwrap_or_default();
                    if source.is_empty() || cs.is_empty() || cs == source { load_chip_value(&wv, cx); }
                }
            }
        }
        "doc" => {
            for k in matching(false) { load(&k, cx); }
            refresh(cx);
        }
        "render" => { if with(|st| st.pane == "checks" && st.selected == id) { load_shots(cx); } }
        _ => {
            refresh(cx);
            if with(|st| st.selected == id && !st.pane.is_empty()) { let p = with(|st| st.pane.clone()); show_pane(&p, cx); }
        }
    }
}

/// Every 33 ms: the pages' new frames. Every 250 ms: the pages' messages,
/// due reloads, copies no longer drawn give their browser back (3 s) and are
/// forgotten (30 s); every 25 s the server hears which panels are on screen.
fn start(cx: &mut App) {
    if with(|st| std::mem::replace(&mut st.started, true)) { return; }
    crundi_workbench::register_kind(cx, crundi_workbench::PaneKind {
        prefix: "panel:widget",
        render: Rc::new(|key, window, cx| view::pane(key, window, cx)),
        title: Rc::new(|key, _| {
            let id = picked_of(key);
            let t = if id.is_empty() { String::new() } else { title_of(&id) };
            (if t.is_empty() { "Panel".to_string() } else { t }, "idle".to_string())
        }),
    });
    cx.spawn(async move |cx: &mut AsyncApp| {
        let mut k: u64 = 0;
        loop {
            cx.background_executor().timer(Duration::from_millis(33)).await;
            k += 1;
            cx.update(|cx| { pump(cx); if k % 8 == 0 { tick(cx); } });
        }
    }).detach();
}

fn pump(cx: &mut App) {
    let pages: Vec<Entity<PageView>> = with(|st| st.views.values().filter(|v| v.panel.is_some()).filter_map(|v| v.page.clone()).collect());
    if pages.is_empty() { return; }
    let Some(h) = cx.windows().first().copied() else { return };
    let _ = h.update(cx, |_, window, cx| for p in &pages { p.update(cx, |p, cx| p.pump(window, cx)); });
}

fn tick(cx: &mut App) {
    let keys: Vec<String> = with(|st| st.views.keys().cloned().collect());
    for k in &keys { read_title(k, cx); }
    let (due, unseen, gone, probe, watch) = with(|st| {
        st.ticks += 1;
        let now = Instant::now();
        let due: Vec<String> = st.views.iter().filter(|(_, v)| v.reload_at.is_some_and(|t| t <= now) && !v.loading).map(|(k, _)| k.clone()).collect();
        let old = |v: &View, secs: u64| v.seen.is_none_or(|t| t.elapsed() > Duration::from_secs(secs));
        // The one rule of every web view: not drawn (after a probe), hidden
        // at once; not drawn for the unload grace, its page freed (drawn
        // again: loaded again).
        let grace = crundi_webview::unload_grace();
        let unseen: Vec<String> = st.views.iter().filter(|(_, v)| v.panel.is_some() && v.seen.is_none_or(|t| t.elapsed() > grace.max(Duration::from_secs(2)))).map(|(k, _)| k.clone()).collect();
        let hide: Vec<usize> = st.views.values_mut().filter(|v| v.panel.is_some() && !v.hidden && old(v, 2)).filter_map(|v| { v.hidden = true; v.panel }).collect();
        for i in hide { engine::set_parked(i, true); }
        let keep = grace.as_secs().max(30) + 5;
        let gone: Vec<String> = st.views.iter().filter(|(_, v)| old(v, keep)).map(|(k, _)| k.clone()).collect();
        let probe = st.views.values().any(|v| v.panel.is_some() && old(v, 1));
        (due, unseen, gone, probe, st.ticks % 100 == 0)
    });
    for k in due {
        let drawn = with(|st| st.views.get(&k).is_some_and(|v| v.seen.is_some_and(|t| t.elapsed() < Duration::from_secs(2))));
        if drawn { load(&k, cx); } else { with(|st| if let Some(v) = st.views.get_mut(&k) { v.reload_at = None; }); release(&k, cx); }
    }
    for k in unseen { release(&k, cx); }
    for k in gone { release(&k, cx); with(|st| st.views.remove(&k)); }
    // A box drawn on a still screen is not laid out again: draw once to see.
    if probe { bump(cx); }
    if watch { keep_alive(cx); }
}

fn keep_alive(cx: &mut App) {
    let (p, list, mut by) = with(|st| {
        let mut by: HashMap<String, Vec<String>> = HashMap::new();
        for v in st.views.values().filter(|v| v.seen.is_some_and(|t| t.elapsed() < Duration::from_secs(2))) {
            let e = by.entry(v.alias.clone()).or_default();
            if !e.contains(&v.id) { e.push(v.id.clone()); }
        }
        (st.project.clone(), st.widgets.clone(), by)
    });
    for wv in list.iter().filter(|w| s(w, "state") == "open" && s(w, "slot") == "chip") {
        let e = by.entry(p.clone()).or_default();
        let id = s(wv, "id");
        if !e.contains(&id) { e.push(id); }
    }
    for (alias, ids) in by {
        let body = json!({ "project": alias, "ids": ids });
        crundi_shell::call(cx, move |api| api.post("/api/widgets/watch", &body), |_, _| {});
    }
}

// ─── The spine (lib.rs) ───

struct Screen;
impl Render for Screen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement { view::tab(window, cx) }
}
struct ScreenHandle(Entity<Screen>);
impl Global for ScreenHandle {}

/// This screen's tab ("" for none): its page.
pub fn view(tab: &str, _window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    if tab != "panels" { return None; }
    let e = match cx.try_global::<ScreenHandle>() { Some(h) => h.0.clone(), None => { let e = cx.new(|_| Screen); cx.set_global(ScreenHandle(e.clone())); e } };
    Some(e.into_any_element())
}

pub fn on_event(e: &AreaEvent, cx: &mut App) {
    match e {
        AreaEvent::Server("widget", d) => on_widget_event(d, cx),
        AreaEvent::Connected(true) => refresh(cx),
        _ => {}
    }
}

pub fn on_project(alias: &str, cx: &mut App) {
    let changed = with(|st| {
        if st.project == alias && st.loaded { return false; }
        st.project = alias.to_string();
        // Global panels belong to every project: they stay while the list loads.
        st.widgets.retain(|w| flag(w, "global"));
        st.chip_values.clear();
        st.pop.clear();
        st.selected.clear();
        st.pane.clear();
        st.cells_sig = String::from("\u{0}");
        true
    });
    if changed { bump(cx); refresh(cx); }
}

pub fn on_signed_in(cx: &mut App) {
    start(cx);
    let p = crundi_shell::project(cx);
    with(|st| st.project = p);
    refresh(cx);
}

pub fn on_signed_out(cx: &mut App) {
    let keys: Vec<String> = with(|st| st.views.keys().cloned().collect());
    for k in &keys { release(k, cx); }
    with(|st| { let started = st.started; *st = St { started, ..Default::default() }; });
    bump(cx);
}

pub fn on_tab(tab: &str, cx: &mut App) { if tab == "panels" { refresh(cx); } }

/// A driver action (Some(answer) for a name it knows).
pub fn test_ui(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let ok = json!({ "ok": true });
    Some(match name {
        "panels-refresh" => { refresh(cx); ok }
        "panels-select" => { select(&a(0), cx); ok }
        "panels-pane" => { show_pane(&a(0), cx); ok }
        "panels-slot" => { slot(&a(0), &a(1), cx); ok }
        "panels-pin" => { pin(&a(0), cx); ok }
        "panels-scope" => { set_scope(&a(0), a(1) == "true", cx); ok }
        "panels-reload" => { show_pane("", cx); reload_id(&a(0), cx); ok }
        "panels-close" => { close(&a(0), cx); ok }
        "panels-reopen" => { reopen(&a(0), cx); ok }
        "panels-delete" => { delete(&a(0), window, cx); ok }
        "panels-restore" => { restore(&a(0), &a(1), cx); ok }
        "panels-approve" => { approve(&a(0), cx); ok }
        "panels-not-now" => { not_now(&a(0), cx); ok }
        "panels-pick" => { pick(&a(0), &a(1), cx); ok }
        "panels-open-tab" => { open_tab(&a(0), cx); ok }
        "panels-chip" => { open_pop(&a(0), cx); ok }
        "panels-pop-close" => { close_pop(cx); ok }
        "panels-collapse" => { collapse(&a(0), a(1) == "true", cx); ok }
        "panels-slot-menu" => { slot_menu(&a(0), point(crundi_theme::T::DIALOG_W, crundi_theme::T::SP_48 * 4.), cx); ok }
        // The driver's look at an inline card (project, id, chat; no id: hide).
        "panels-card" => { with(|st| st.preview = (a(0), a(1), a(2))); bump(cx); ok }
        "panels-cells" => json!({ "ok": true, "cells": cell_ids(cx), "swaps": with(|st| st.picked.iter().map(|(k, v)| (k.clone(), Value::from(v.0.clone()))).collect::<serde_json::Map<_, _>>()) }),
        _ => return None,
    })
}

/// The driver's view of this screen (the Slint client's names).
pub fn props(_window: &mut Window, cx: &mut App) -> Value {
    let live = live_chats(&with(|st| st.project.clone()), cx);
    with(|st| {
        let views: serde_json::Map<String, Value> = st.views.iter().map(|(k, v)| {
            let (lv, pt) = panel_live(v.panel);
            (k.clone(), json!({
                "state": v.state, "error": v.error, "grant": v.grant.is_some() && !v.grant_hidden, "grantItems": v.grant.as_ref().map(|g| g.items.len()).unwrap_or(0),
                "height": v.height, "measured": v.measured, "chipW": v.chip_w, "title": v.title, "loads": v.nonce, "msgs": v.msgs,
                "live": lv, "painted": pt, "w": v.size.0, "h": v.size.1,
            }))
        }).collect();
        json!({
            "project": st.project, "loaded": st.loaded, "error": st.error,
            "widgets": st.widgets.iter().map(|w| json!({ "id": s(w, "id"), "title": s(w, "title"), "slot": s(w, "slot"), "state": s(w, "state"), "place": place_of(w, &live), "global": flag(w, "global"), "needsGrant": flag(w, "needsGrant"), "lifecycle": s(w, "lifecycle"), "collapsed": flag(w, "collapsed") })).collect::<Vec<_>>(),
            "order": tab_order(&st.widgets).iter().map(|w| s(w, "id")).collect::<Vec<_>>(),
            "selected": st.selected, "pane": st.pane,
            "versions": st.versions.iter().map(|v| s(v, "version")).collect::<Vec<_>>(), "versionsState": st.versions_state,
            "shots": st.shots.iter().map(|x| json!({ "caption": x.0, "ready": x.1.is_some() })).collect::<Vec<_>>(), "checksState": st.checks_state,
            "picked": st.picked.iter().map(|(k, v)| (k.clone(), Value::from(v.0.clone()))).collect::<serde_json::Map<_, _>>(),
            "chipValues": st.chip_values, "pop": st.pop,
            "views": views, "opened": st.opened, "lastCall": st.last_call,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{build_html, chip_text, frame_name, image_size, is_inline_card, item_badge, item_sub, parse_key, parse_title, path_get, pick_status, pkey, place_of, s, n, scope_of, slot_options, tab_order, enc, TITLE_MARK};
    use serde_json::json;

    #[::core::prelude::v1::test]
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
        assert_eq!(enc("a b/é"), "a%20b%2F%C3%A9");
    }

    #[::core::prelude::v1::test]
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

    #[::core::prelude::v1::test]
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
        assert_eq!(item_badge(&json!({ "faults": 2 })), ("error", "error"));
        assert_eq!(item_badge(&json!({})), ("", ""));
    }

    #[::core::prelude::v1::test]
    fn order_and_scope() {
        let list = vec![
            json!({ "id": "a", "state": "closed", "updatedAt": 30 }),
            json!({ "id": "b", "state": "open", "updatedAt": 10 }),
            json!({ "id": "c", "state": "open", "updatedAt": 20 }),
        ];
        let ids: Vec<String> = tab_order(&list).iter().map(|w| s(w, "id")).collect();
        assert_eq!(ids, vec!["c", "b", "a"]);
        assert_eq!(scope_of(&json!({}), "demo"), "local");
        assert_eq!(scope_of(&json!({ "global": true, "home": "" }), "demo"), "global");
        assert_eq!(scope_of(&json!({ "global": true, "home": "demo" }), "Demo"), "global");
        assert_eq!(scope_of(&json!({ "global": true, "home": "other" }), "demo"), "other");
        assert_eq!(s(&json!({ "version": 3 }), "version"), "3");
    }

    #[::core::prelude::v1::test]
    fn chip_values() {
        let v = json!({ "build": { "pct": 42, "name": "x" } });
        assert_eq!(chip_text(path_get(&v, "build.pct")), "42");
        assert_eq!(chip_text(path_get(&v, "build.none")), "");
        assert_eq!(chip_text(path_get(&json!("ok"), "")), "ok");
        assert_eq!(chip_text(Some(&json!("x".repeat(80)))).len(), 60);
    }

    #[::core::prelude::v1::test]
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
        assert_eq!(n(&m[1].1, "h"), 120.0);
        assert!(parse_title(&title, 4).is_none(), "an older load's title is not read");
        assert!(parse_title("Some page", 3).is_none());
        assert!(is_inline_card("mcp__crundi__widget_open", &json!({ "slot": "inline", "id": "x" })));
        assert!(!is_inline_card("mcp__crundi__widget_open", &json!({ "slot": "cell", "id": "x" })));
        let mut png = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        png.extend_from_slice(&640u32.to_be_bytes());
        png.extend_from_slice(&400u32.to_be_bytes());
        assert_eq!(image_size(&png), (640.0, 400.0));
        assert_eq!(image_size(b"jpeg"), (16.0, 10.0));
    }
}
