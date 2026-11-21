//! Factories (4.8): the live graph of a run (tools/factory-flow/src/Viewer.jsx).
//! One at a time: the tab's (with Back, the minimap, the replay bar) or the
//! panel's (Follow, Details as a drawer, a transcript over the graph): never
//! both on show. It reads the run (/api/factory-runs/get: the summary, the
//! factory's snapshot and the history) and its journal (/events, only what
//! is new), on the server's factoryRun event and every few seconds while on
//! show (a missed push must not leave it behind).
//!
//! The web folds the journal with the server's reducer to replay any
//! moment; so does this client (journal.rs, on the same engine as the Dry
//! run): a past moment shows its tokens (who was working, waiting, asked),
//! its asks and its history as they were then. Live, it draws the run from
//! the server's record.
//!
//! What ticks while on show: the run's duration with the poll (every 3 s,
//! as the web's re-render on each poll) and a Wait's countdown every second
//! (the web's WaitCard), each drawing only when its words change.
//!
//! A person's transcript is their live chat (the web's /chat/<id> page in a
//! frame): the chat area's log and box (crundi_chat), on a store of its own
//! that this area subscribes and feeds from the live events (`route_chat`).

use super::model::{self, arr, b, n, s, truthy, Rect};
use super::parts::{self, btn, kv, note, AskRow, Edge, Kv, Node, Opt, Out, StepRow, Tl, Wait, Worker};
use super::{act as fact, with as fwith, Fx};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet}, rc::Rc};

/// The run graph's lowest opening zoom: its step text stays readable (F78).
const READABLE_Z: f32 = 0.75;

/// The Slint client's PlanningFxView: what the graph's bars and panes show.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Vx {
    pub state: String, pub error: String, pub panel: bool, pub back: bool, pub name: String, pub live: bool,
    pub st: String, pub st_label: String, pub progress: String, pub follow: bool, pub drawer: bool, pub chat: bool,
    pub can_pause: bool, pub can_resume: bool, pub can_stop: bool, pub busy: bool,
    pub pause_menu: bool, pub pause_reset: bool, pub pause_opts: bool,
    pub needs_kind: String, pub needs_text: String, pub needs_act: String,
    pub tab: String, pub sel: String, pub sel_title: String, pub err: String, pub hint: String,
    pub events: i32, pub at: i32, pub at_label: String,
    pub who: String, pub now: String, pub visits: String, pub open_chat: String, pub no_chat: String,
    pub has_limits: bool, pub limits_open: bool, pub limits_sum: String, pub l5h: String, pub lweek: String, pub lpar: String, pub lone: String,
    pub manager_talks: bool, pub manager_chat: bool,
    pub findings: String, pub crashed: bool, pub crash: String, pub cap: String, pub note: String,
    pub zoom: f32, pub pan_x: f32, pub pan_y: f32,
    pub bx: f32, pub by: f32, pub bw: f32, pub bh: f32,
}

#[derive(Default)]
pub(crate) struct V {
    open: bool,
    id: String,
    panel: bool,
    events: Vec<Value>,
    run: Option<Value>,
    at: Option<usize>,
    sel: String,
    tab: String,
    follow: bool,
    drawer: bool,
    chat: Option<(String, String)>,
    busy: bool,
    err: String,
    note: String,
    ask_note: String,
    pause_menu: bool,
    pause_reset: bool,
    pause_opts: bool,
    limits_open: bool,
    limits: Value,
    findings_open: bool,
    /// The poll while the graph is on show (F52): none while it is hidden.
    timer: Option<Task<()>>,
    /// A Wait's countdown (every second) while one is on show, live.
    wait_timer: Option<Task<()>>,
    /// The journal folded (journal.rs): recs[i] is the run after i events
    /// (recs[0] none), made as far as a replay asked.
    recs: Vec<Value>,
    z: f32, px: f32, py: f32, cw: f32, ch: f32,
    fitted: bool,
    active_key: String,
    counts: HashMap<String, i64>,
    pulse: HashSet<String>,
    rects: Vec<(String, Rect)>,
    gn: u64,
    state: String,
    error: String,
    fetching: bool,
    moved: bool,
    base: String,
    // What it shows.
    ui: Vx,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    working: Vec<Worker>,
    kv: Vec<Kv>,
    asks: Vec<AskRow>,
    waits: Vec<Wait>,
    findings: Vec<String>,
    timeline: Vec<Tl>,
    steps: Vec<StepRow>,
}

pub(crate) fn blank() -> V { V { z: 1.0, tab: "run".into(), ..Default::default() } }

fn with<R>(cx: &mut App, f: impl FnOnce(&mut V) -> R) -> R { super::with_fx(cx, |x| f(&mut x.v)) }
/// While drawing: the graph's state and the tab's, from the store being drawn.
fn rv<R>(fx: &mut Fx, f: impl FnOnce(&mut V) -> R) -> R { f(&mut fx.v) }

/// Every few seconds while the graph is on show (ACTIVITY_MS): a missed push
/// must not leave it behind. The poll runs only while the graph shows (F52):
/// started when it is shown, dropped when it is hidden or closed; with no
/// graph on show there is no timer at all.
pub fn sync_timer(cx: &mut App) {
    let on = showing(cx);
    let has = with(cx, |v| v.timer.is_some());
    if on == has { return; }
    if !on { with(cx, |v| v.timer = None); return; }
    let t = cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor().timer(std::time::Duration::from_millis(3000)).await;
        let go = cx.update(|cx| { if showing(cx) { tick_progress(cx); fetch(cx); true } else { with(cx, |v| { v.timer = None; v.wait_timer = None; }); false } });
        if !go { break; }
    });
    with(cx, |v| v.timer = Some(t));
}

/// The run's duration as the web's progress line reads it now (Date.now()
/// at each poll's re-render): drawn again only when its words changed.
fn tick_progress(cx: &mut App) {
    let changed = with(cx, |v| {
        let Some(run) = v.run.as_ref() else { return false };
        if v.ui.state != "ok" { return false; }
        let p = model::progress(run, model::now_ms());
        if p == v.ui.progress { return false; }
        v.ui.progress = p;
        true
    });
    if changed { super::bump(cx); }
}

/// The Waits in progress as the run pane shows them (WaitCard), at `now`.
fn waits_of(run: &Value, now: f64, base: &str) -> Vec<Wait> {
    arr(run, "tokens").iter().filter(|t| t.get("wait").map(|w| w.is_object()).unwrap_or(false)).map(|t| {
        let w = &t["wait"];
        let two = { let k = s(w, "kind"); k == "command" || k == "signal" };
        let sp = s(w, "signalPath");
        Wait { token: s(t, "id"), head: model::wait_head(t, now, false), gives: if two { format!("Gives up in {}.", model::left(&s(w, "timeoutAt"), now)) } else { String::new() },
            last: if s(w, "kind") == "command" { s(w, "last") } else { String::new() }, url: if sp.is_empty() { String::new() } else { format!("{base}{sp}") }, two }
    }).collect()
}

/// A Wait's countdown ticks every second while one shows, live (WaitCard's
/// interval); no timer otherwise. Drawn only when its words change.
fn sync_wait_timer(cx: &mut App) {
    let want = showing(cx) && with(cx, |v| v.at.is_none() && !v.waits.is_empty());
    let has = with(cx, |v| v.wait_timer.is_some());
    if want == has { return; }
    if !want { with(cx, |v| v.wait_timer = None); return; }
    let t = cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor().timer(std::time::Duration::from_millis(1000)).await;
        let go = cx.update(|cx| {
            if !showing(cx) || with(cx, |v| v.at.is_some() || v.waits.is_empty()) { with(cx, |v| v.wait_timer = None); return false; }
            let changed = with(cx, |v| {
                let Some(run) = v.run.as_ref() else { return false };
                let now = waits_of(run, model::now_ms(), &v.base);
                if now == v.waits { return false; }
                v.waits = now;
                true
            });
            if changed { super::bump(cx); }
            true
        });
        if !go { break; }
    });
    with(cx, |v| v.wait_timer = Some(t));
}

pub fn reset(cx: &mut App) { with(cx, |v| *v = blank()); }
pub fn run_id(cx: &mut App) -> String { with(cx, |v| v.id.clone()) }
pub fn is_panel(cx: &mut App) -> bool { with(cx, |v| v.open && v.panel) }

/// Whether the graph is on show where it lives (its tab, or the panel on the workbench).
pub fn showing(cx: &mut App) -> bool {
    let (open, panel) = with(cx, |v| (v.open, v.panel));
    if !open { return false; }
    if panel { super::panel_visible(cx) } else { super::tab_visible(cx) && fwith(cx, |s| s.tab.sub == "viewer") }
}

/// Show a run's graph (the tab's, or the panel's).
pub fn open(id: &str, panel: bool, cx: &mut App) {
    let same = with(cx, |v| v.open && v.id == id && v.panel == panel);
    if !same {
        // The canvas reports its own size (the tab's and the panel's differ): fit after that.
        let base = parts::base(cx);
        let (cw, ch) = parts::bounds_in(cx, "cv-v").map(|b| (f32::from(b.size.width), f32::from(b.size.height))).unwrap_or((0.0, 0.0));
        with(cx, |v| { *v = V { open: true, id: id.to_string(), panel, z: 1.0, tab: "run".into(), follow: panel, state: "loading".into(), base, cw, ch, ..Default::default() }; });
        build(cx);
        super::caps(cx, |c, cx| { with(cx, |v| v.pause_opts = c.iter().any(|x| x == "factory-pause-options")); });
    }
    fetch(cx);
    sync_timer(cx);
}

pub fn close(cx: &mut App) {
    with(cx, |v| { v.open = false; v.chat = None; v.ui.chat = false; v.timer = None; v.wait_timer = None; });
}

/// The journal (only what is new) and the run's record, then drawn.
pub fn fetch(cx: &mut App) {
    let (id, from, gn, busy) = with(cx, |v| { let b = v.fetching; v.fetching = true; (v.id.clone(), v.events.len(), v.gn, b) });
    if id.is_empty() || busy { if id.is_empty() { with(cx, |v| v.fetching = false); } return; }
    let ep = format!("/api/factory-runs/events?id={}&from={from}", parts::enc(&id));
    let rp = format!("/api/factory-runs/get?id={}", parts::enc(&id));
    crundi_shell::call(cx, move |api| {
        let e = api.get(&ep).unwrap_or_else(|e| json!({ "ok": false, "error": e.to_string() }));
        let r = api.get(&rp).unwrap_or_else(|e| json!({ "ok": false, "error": e.to_string() }));
        Ok(json!({ "e": e, "r": r }))
    }, move |res, cx| {
        with(cx, |v| v.fetching = false);
        let Ok(x) = res else { return };
        if with(cx, |v| v.id != id || v.gn != gn || !v.open) { return; }
        let (e, r) = (&x["e"], &x["r"]);
        let mut again = false;
        // What the graph showed: an answer that changes nothing draws nothing
        // (the timer's fetch, every few seconds while on show, is usually that).
        let before = with(cx, |v| (v.run.clone(), v.events.len(), v.state.clone(), v.error.clone()));
        with(cx, |v| {
            if b(r, "ok") { v.run = Some(r["run"].clone()); v.state = "ok".into(); }
            else if v.run.is_none() { let er = parts::err_text(r, ""); if !er.is_empty() { v.state = "error".into(); v.error = er; } }
            if b(e, "ok") {
                let evs = arr(e, "events");
                if !evs.is_empty() && n(e, "from") as usize == v.events.len() { v.events.extend(evs.iter().cloned()); }
                else if (n(e, "total") as usize) < v.events.len() { v.events.clear(); v.recs.clear(); again = true; }
            }
        });
        if with(cx, |v| (v.run.clone(), v.events.len(), v.state.clone(), v.error.clone())) != before {
            build(cx);
            super::bump(cx);
        }
        if again { fetch(cx); }
    });
}

// ─── Building what it shows ───

fn names_of(f: &Value) -> HashMap<String, String> { arr(f, "roster").iter().map(|r| (s(r, "id"), s(r, "name"))).collect() }

/// The history's handoffs, oldest first.
fn handoffs(history: &[Value]) -> Vec<&Value> { history.iter().filter(|h| s(h, "type") == "handoff").collect() }


/// Edge use, from the order the steps handed off in.
fn edge_counts(f: &Value, seq: &[String], active: &[String]) -> HashMap<String, i64> {
    let mut pairs: HashMap<(String, String), i64> = HashMap::new();
    let start = arr(f, "nodes").iter().find(|x| s(x, "type") == "start").map(|x| s(x, "id"));
    let mut path: Vec<String> = start.into_iter().collect();
    path.extend(seq.iter().cloned());
    for w in path.windows(2) { *pairs.entry((w[0].clone(), w[1].clone())).or_default() += 1; }
    if let Some(last) = path.last() { for a in active { if a != last { *pairs.entry((last.clone(), a.clone())).or_default() += 1; } } }
    arr(f, "edges").iter().filter(|e| s(e, "kind") == "flow").map(|e| (s(e, "id"), pairs.get(&(s(e, "source"), s(e, "target"))).copied().unwrap_or(0))).collect()
}

pub fn build(cx: &mut App) {
    let st = with(cx, |v| (v.state.clone(), v.error.clone(), v.panel));
    if st.0 != "ok" {
        with(cx, |v| { v.ui.state = st.0.clone(); v.ui.error = st.1.clone(); v.ui.panel = st.2; v.ui.back = !st.2; });
        return;
    }
    let now = model::now_ms();
    let (run, events, at, sel, tab, panel, busy) = with(cx, |v| (v.run.clone().unwrap_or(Value::Null), v.events.clone(), v.at, v.sel.clone(), v.tab.clone(), v.panel, v.busy));
    let total = events.len();
    let nn = at.unwrap_or(total).min(total);
    let live = at.is_none();
    // A past moment: the journal folded up to it (the web's recs[n]).
    let rec = if live { Value::Null } else { with(cx, |v| {
        if v.recs.is_empty() { v.recs.push(Value::Null); }
        while v.recs.len() <= nn {
            let i = v.recs.len();
            let next = super::journal::reduce(if v.recs[i - 1].is_null() { None } else { Some(&v.recs[i - 1]) }, &v.events[i - 1]);
            v.recs.push(next);
        }
        v.recs[nn].clone()
    }) };
    let f = if !live && rec["state"]["snapshot"].is_object() { rec["state"]["snapshot"].clone() } else { run.get("snapshot").cloned().unwrap_or(json!({ "nodes": [], "edges": [], "roster": [] })) };
    let history: Vec<Value> = if live { arr(&run, "history").to_vec() } else { arr(&rec["state"], "history").to_vec() };
    let names = names_of(&f);
    let nodes_by: HashMap<String, Value> = arr(&f, "nodes").iter().map(|x| (s(x, "id"), x.clone())).collect();
    let status = s(&run, "status");
    let quiet = ["crashed", "paused", "stopped", "finished"].contains(&status.as_str());

    let hs = handoffs(&history);
    let done: Vec<&Value> = hs.clone();
    // Live, the server's tokens; a past moment, that moment's (the fold's).
    let tokens: Vec<Value> = if live { arr(&run, "tokens").to_vec() } else { arr(&rec["state"], "tokens").to_vec() };
    let mut tokens_at: HashMap<String, Vec<Value>> = HashMap::new();
    for t in &tokens { if !s(t, "nodeId").is_empty() && s(t, "job").is_empty() { tokens_at.entry(s(t, "nodeId")).or_default().push(t.clone()); } }
    let visited: HashSet<String> = done.iter().map(|h| s(h, "nodeId")).collect();
    let mut skills_at: HashMap<String, Vec<String>> = HashMap::new();
    for h in &done { for k2 in arr(h, "skills") { if let Some(x) = k2.as_str() { let e = skills_at.entry(s(h, "nodeId")).or_default(); if !e.iter().any(|y| y == x) { e.push(x.to_string()); } } } }
    let employees = run.get("employees").cloned().unwrap_or(json!({}));
    // Who was stopped for now: live, the server's; a past moment, the fold's.
    let retired_of = |who: &str| if live { b(&employees[who], "retired") } else { b(&rec["run"]["retired"], who) };

    let mut out_nodes: Vec<Node> = vec![];
    let mut rects: Vec<(String, Rect)> = vec![];
    let mut outs_of: HashMap<String, Vec<String>> = HashMap::new();
    let mut job_nodes: Vec<(String, String)> = vec![];
    for node in arr(&f, "nodes") {
        let id = s(node, "id");
        let ty = { let t = s(node, "type"); if ["start", "manager", "employee", "decision", "join", "ask", "wait", "end"].contains(&t.as_str()) { t } else { "employee".into() } };
        let who = model::person_of(node);
        let mut toks = tokens_at.get(&id).cloned().unwrap_or_default();
        let blocked = toks.iter().any(|t| b(t, "blocked"));
        let mut body = model::node_body(node, &f);
        if let Some(last) = done.iter().rev().find(|h| s(h, "nodeId") == id) { let sm = s(last, "summary"); if !sm.is_empty() { body = model::first(&sm, 110); } }
        let (mut doing, mut doing_who, mut doing_text, mut doing_busy) = (false, String::new(), String::new(), false);
        let mut doing_chat_of = false;
        if live && !quiet {
            if let Some(tok) = toks.iter().find(|t| { let x = s(t, "status"); x == "work" || x == "answer" }) {
                let emp = &employees[s(tok, "employeeId")];
                let act = &emp["activity"];
                doing = true;
                doing_who = { let e = s(tok, "employee"); if e.is_empty() { names.get(&s(tok, "employeeId")).cloned().unwrap_or_default() } else { e } };
                doing_text = { let t = s(act, "text"); if !t.is_empty() { t } else if s(tok, "status") == "answer" { "Deciding".into() } else { "Starting".into() } };
                doing_busy = s(act, "state") == "working";
                doing_chat_of = !s(emp, "chatId").is_empty();
            } else if !who.is_empty() {
                if let Some(job) = tokens.iter().find(|t| !s(t, "job").is_empty() && s(t, "employeeId") == who && s(t, "status") == "work") {
                    if !job_nodes.iter().any(|(j, _)| *j == s(job, "id")) {
                        let e = &employees[who.as_str()];
                        let at2 = s(&e["activity"], "text");
                        doing = true;
                        doing_who = { let x = s(job, "employee"); if x.is_empty() { names.get(&who).cloned().unwrap_or_default() } else { x } };
                        doing_text = if at2.is_empty() { format!("Job: {}", model::first(&s(job, "job"), 60)) } else { format!("Job: {at2}") };
                        doing_busy = s(&e["activity"], "state") == "working";
                        doing_chat_of = !s(e, "chatId").is_empty();
                        job_nodes.push((s(job, "id"), id.clone()));
                        toks.push(json!({ "id": s(job, "id"), "status": "work" }));
                    }
                }
            }
        }
        let outs = model::outputs_of(node);
        let skills = skills_at.get(&id).map(|l| l.iter().map(|k| format!("/{}", k.rsplit(':').next().unwrap_or(k))).collect::<Vec<_>>().join("  ")).unwrap_or_default();
        let w = model::node_w(&ty);
        let text_for_lines = if doing { format!("{doing_who} {doing_text}") } else { body.clone() };
        let lines = model::body_lines(&text_for_lines, w);
        let h = model::node_h(lines, !skills.is_empty(), outs.len());
        let x = n(&node["position"], "x") as f32;
        let y = n(&node["position"], "y") as f32;
        rects.push((id.clone(), Rect { x, y, w, h }));
        let (token, token_kind) = model::token_pill(&toks, blocked);
        out_nodes.push(Node {
            id: id.clone(), kind: ty.clone(), kind_label: model::kind_label(&ty).to_uppercase(), title: model::node_title(node, &f), body: body.clone(), lines,
            x, y, w, h, sel: sel == id, bad: false, active: !toks.is_empty(), visited: visited.contains(&id) && toks.is_empty(),
            retired: !who.is_empty() && retired_of(&who), badge: 0, badge_level: String::new(), token, token_kind,
            doing, doing_who, doing_text, doing_busy, skills,
            outs: outs.iter().map(|h2| Out { id: h2.clone(), label: model::handle_label(node, h2), finish: h2 == "finish" }).collect(),
            agent: model::is_agent(&ty), has_in: ty != "start", doing_chat: doing_chat_of,
            ..Default::default()
        });
        outs_of.insert(id.clone(), outs);
    }

    let seq: Vec<String> = done.iter().map(|h| s(h, "nodeId")).collect();
    let active_ids: Vec<String> = tokens_at.keys().cloned().collect();
    let counts = edge_counts(&f, &seq, &active_ids);
    let finished = if live { !run.get("outcome").map(|o| o.is_null()).unwrap_or(true) } else { truthy(&rec["state"]["outcome"]) };
    let prev = with(cx, |v| std::mem::replace(&mut v.counts, counts.clone()));
    let pulse: HashSet<String> = counts.iter().filter(|(k2, c)| **c > prev.get(*k2).copied().unwrap_or(0) && !prev.is_empty()).map(|(k2, _)| k2.clone()).collect();
    let lanes = model::lanes(&f);
    let rect_of: HashMap<String, Rect> = rects.iter().cloned().collect();
    let mut out_edges: Vec<Edge> = vec![];
    for e in arr(&f, "edges") {
        let (src, tgt) = (s(e, "source"), s(e, "target"));
        let (Some(sr), Some(tr)) = (rect_of.get(&src), rect_of.get(&tgt)) else { continue };
        let note = s(e, "kind") == "note";
        let id = s(e, "id");
        let fin = s(e, "sourceHandle") == "finish";
        let used = if fin { if finished { 1 } else { 0 } } else { counts.get(&id).copied().unwrap_or(0) };
        let sh_name = s(e, "sourceHandle");
        let (sx, sy) = model::out_point(sr, outs_of.get(&src).map(|v| v.as_slice()).unwrap_or(&[]), if note { "note" } else { sh_name.as_str() });
        let (tx, ty) = model::in_point(tr, note);
        let (path, lx, ly) = match lanes.get(&id) { Some(l) if !note => model::loop_path(sx, sy, tx, ty, *l), _ => model::bezier(sx, sy, tx, ty, note) };
        let label = model::edge_label(e, Some(used));
        let tone = if used > 0 { "used" } else if n(e, "maxRounds") > 0.0 && !note { "loop" } else if note { "note" } else { "flow" };
        out_edges.push(Edge {
            id: id.clone(), path, arrow: model::arrow(tx, ty, note), tone: tone.into(), sel: false, pulse: pulse.contains(&id) || (fin && finished && prev.is_empty()),
            label, icon: if n(e, "maxRounds") > 0.0 && !note { "loop".into() } else if note { "note".into() } else { String::new() }, lx, ly,
        });
    }
    if live {
        if let Some(mgr) = arr(&f, "nodes").iter().find(|x| s(x, "type") == "manager").map(|x| s(x, "id")) {
            for t in tokens.iter().filter(|t| !s(t, "job").is_empty() && s(t, "status") == "work") {
                let target = job_nodes.iter().find(|(j, _)| *j == s(t, "id")).map(|(_, nid)| nid.clone())
                    .or_else(|| arr(&f, "nodes").iter().find(|x| s(x, "type") == "employee" && s(x, "employeeId") == s(t, "employeeId")).map(|x| s(x, "id")));
                let (Some(target), Some(sr)) = (target, rect_of.get(&mgr)) else { continue };
                let Some(tr) = rect_of.get(&target) else { continue };
                let (sx, sy) = model::out_point(sr, outs_of.get(&mgr).map(|v| v.as_slice()).unwrap_or(&[]), "out");
                let (tx, ty) = model::in_point(tr, false);
                let (path, lx, ly) = model::bezier(sx, sy, tx, ty, false);
                out_edges.push(Edge { id: format!("job-{}", s(t, "id")), path, arrow: model::arrow(tx, ty, false), tone: "job".into(), label: "Job".into(), lx, ly, ..Default::default() });
            }
        }
    }
    let bounds = model::union(&rects.iter().map(|r| r.1).collect::<Vec<_>>()).unwrap_or(Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 });
    with(cx, |v| { v.rects = rects.clone(); v.pulse = pulse; });

    // Follow the work (the panel), or fit once.
    let active_key = if live {
        let mut ids: Vec<String> = arr(&run, "tokens").iter().filter_map(|t| {
            let st2 = s(t, "status");
            if !s(t, "nodeId").is_empty() && ["work", "answer", "ask", "wait", "join"].contains(&st2.as_str()) { Some(s(t, "nodeId")) }
            else if !s(t, "job").is_empty() && st2 == "work" { arr(&f, "nodes").iter().find(|x| s(x, "type") == "employee" && s(x, "employeeId") == s(t, "employeeId")).map(|x| s(x, "id")) }
            else { None }
        }).collect();
        ids.sort(); ids.dedup(); ids.join(",")
    } else { String::new() };
    let (follow, fitted, old_key, cw, ch) = with(cx, |v| (v.follow, v.fitted, v.active_key.clone(), v.cw, v.ch));
    if cw > 0.0 && (!fitted || (follow && active_key != old_key)) {
        let pick: Vec<Rect> = if follow && !active_key.is_empty() { active_key.split(',').filter_map(|id| rect_of.get(id).copied()).collect() } else { vec![] };
        let (z, px, py) = if !pick.is_empty() { model::fit(model::union(&pick).unwrap(), cw, ch, 0.45, 0.15, 0.95) } else {
            // F78: a long graph fitted whole is unreadable (6 px text): open at
            // a readable floor instead, from the graph's top (the run's start).
            let (z, px, py) = model::fit(bounds, cw, ch, 0.15, 0.15, 1.0);
            if z >= READABLE_Z { (z, px, py) } else { (READABLE_Z, cw / 2.0 - (bounds.x + bounds.w / 2.0) * READABLE_Z, ch * 0.06 - bounds.y * READABLE_Z) }
        };
        with(cx, |v| { v.z = z; v.px = px; v.py = py; v.fitted = true; v.active_key = active_key.clone(); });
    } else { with(cx, |v| v.active_key = active_key.clone()); }

    let (kv, asks, waits, findings, steps, timeline, working, mut vx) = side(cx, &run, &rec, &f, &history, &events, nn, live, &sel, &names, &nodes_by, &tokens, now, panel);
    vx.tab = tab;
    vx.busy = busy;
    vx.bx = bounds.x; vx.by = bounds.y; vx.bw = bounds.w; vx.bh = bounds.h;
    with(cx, |v| {
        vx.zoom = v.z; vx.pan_x = v.px; vx.pan_y = v.py;
        v.ui = vx; v.nodes = out_nodes; v.edges = out_edges; v.kv = kv; v.asks = asks; v.waits = waits; v.findings = findings; v.steps = steps; v.timeline = timeline; v.working = working;
    });
    sync_wait_timer(cx);
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn side(cx: &mut App, run: &Value, rec: &Value, f: &Value, history: &[Value], events: &[Value], nn: usize, live: bool, sel: &str, names: &HashMap<String, String>, nodes_by: &HashMap<String, Value>, tokens: &[Value], now: f64, panel: bool)
    -> (Vec<Kv>, Vec<AskRow>, Vec<Wait>, Vec<String>, Vec<StepRow>, Vec<Tl>, Vec<Worker>, Vx) {
    let st = s(run, "status");
    let quiet = ["crashed", "paused", "stopped", "finished"].contains(&st.as_str());
    let employees = run.get("employees").cloned().unwrap_or(json!({}));
    let (follow, drawer, chat, err, note, pause_menu, pause_reset, pause_opts, limits_open, limits, base) = with(cx, |v| (v.follow, v.drawer, v.chat.is_some(), v.err.clone(), v.note.clone(), v.pause_menu, v.pause_reset, v.pause_opts, v.limits_open, v.limits.clone(), v.base.clone()));
    let mut vx = Vx { state: "ok".into(), panel, back: !panel, name: s(f, "name"), live, follow, drawer, chat, err, note, pause_menu: pause_menu && live && !truthy(&run["paused"]), pause_reset, pause_opts, limits_open, ..Default::default() };
    vx.st = st.clone();
    vx.st_label = if live { model::status_word(&st) } else { "Replay".into() };
    vx.progress = model::progress(run, now);
    if live {
        let paused = truthy(&run["paused"]);
        vx.can_pause = (st == "running" || st == "waiting") && !paused;
        vx.can_resume = paused;
        vx.can_stop = ["running", "waiting", "paused", "crashed", "finishing"].contains(&st.as_str());
        let mgr_chat = s(run, "managerChatId");
        let asks = arr(run, "asks");
        let (k, text, act) = if st == "crashed" { ("bad", format!("It stopped: {}. Recover picks it up where it was.", { let r = s(&run["crash"], "reason"); if r.is_empty() { "it was interrupted".into() } else { r } }), "Recover".to_string()) }
            else if st == "finishing" { ("you", "Done. The result is in the Manager's chat: merge it, ask for changes, keep the branch or discard it.".into(), if mgr_chat.is_empty() { String::new() } else { "Open the result".into() }) }
            else if truthy(&run["capHit"]) { ("you", format!("It reached its limit ({}).", model::cap_text(&run["capHit"])), "Let it carry on".into()) }
            else if !asks.is_empty() { ("you", format!("A question for you: {}", s(&asks[0], "prompt")), "Answer".into()) }
            else if s(run, "waitingOn") == "manager" { ("you", "The Manager is talking to you in its chat.".into(), if mgr_chat.is_empty() { String::new() } else { "Open its chat".into() }) }
            else if paused { ("info", model::paused_text(&run["paused"]), "Resume".into()) }
            else { ("", String::new(), String::new()) };
        vx.needs_kind = k.into(); vx.needs_text = text; vx.needs_act = act;
    }
    let mut working: Vec<Worker> = vec![];
    if live && !quiet {
        for t in tokens.iter().filter(|t| { let x = s(t, "status"); x == "work" || x == "answer" }) {
            let e = &employees[s(t, "employeeId")];
            let what = if b(e, "held") { "Held by the Manager".to_string() } else if !s(t, "job").is_empty() { format!("Job: {}", model::first(&s(t, "job"), 50)) } else { let a = s(&e["activity"], "text"); if !a.is_empty() { a } else if s(t, "status") == "answer" { "Deciding".into() } else { "Starting".into() } };
            let blocked = b(t, "blocked");
            working.push(Worker { id: s(t, "id"), name: { let x = s(t, "employee"); if x.is_empty() { names.get(&s(t, "employeeId")).cloned().unwrap_or_else(|| "Someone".into()) } else { x } },
                what: if blocked { "Needs permission".into() } else { what }, busy: s(&e["activity"], "state") == "working", blocked, chat: s(e, "chatId"), queued: false });
        }
    }
    if live { for x in arr(run, "waitingForSeat") { working.push(Worker { id: s(x, "tokenId"), name: s(x, "employee"), what: "Waiting its turn".into(), queued: true, ..Default::default() }); } }

    // Task and Handoffs as of the moment shown (the web's rec.state).
    let at_run = if live { run } else { &rec["state"] };
    let mut kv = vec![Kv { k: "Task".into(), v: s(at_run, "task") }, Kv { k: "Handoffs".into(), v: (n(at_run, "handoffs") as i64).to_string() }];
    if !s(run, "branch").is_empty() { kv.push(Kv { k: "Branch".into(), v: s(run, "branch") }); }
    if live && run.get("liveSessions").map(|x| !x.is_null()).unwrap_or(false) { kv.push(Kv { k: "Sessions".into(), v: format!("{} live", n(run, "liveSessions") as i64) }); }
    let u = &run["usage"];
    if n(u, "turns") > 0.0 { let c = n(u, "costUsd"); kv.push(Kv { k: "Spent".into(), v: format!("{}{} turns in {} chats", if c > 0.0 { format!("${c:.2} · ") } else { String::new() }, n(u, "turns") as i64, n(u, "chats") as i64) }); }
    let lim = &run["limits"];
    if live && lim.is_object() {
        vx.has_limits = true;
        vx.limits_sum = format!("Pause at {}% / {}% · {}{}", n(lim, "pauseAt5h") as i64, n(lim, "pauseAtWeek") as i64,
            if n(lim, "maxParallel") > 0.0 { format!("at most {} at once", n(lim, "maxParallel") as i64) } else { "no limit on people".into() },
            if n(lim, "oneAtATimeAbove") > 0.0 { format!(" · one at a time above {}%", n(lim, "oneAtATimeAbove") as i64) } else { String::new() });
        let l = if limits.is_object() && limits_open { limits } else { lim.clone() };
        vx.l5h = (n(&l, "pauseAt5h") as i64).to_string(); vx.lweek = (n(&l, "pauseAtWeek") as i64).to_string();
        vx.lpar = (n(&l, "maxParallel") as i64).to_string(); vx.lone = (n(&l, "oneAtATimeAbove") as i64).to_string();
    }
    vx.manager_talks = live && s(run, "waitingOn") == "manager";
    vx.manager_chat = live && !s(run, "managerChatId").is_empty();
    // The questions: live, the server's; a past moment, that moment's
    // (asksOf), drawn without the replies to pick (the run pane's `live`).
    let ask_list: Vec<Value> = if live { arr(run, "asks").to_vec() } else { super::journal::asks_of(rec) };
    let asks: Vec<AskRow> = ask_list.iter().map(|a| AskRow { token: s(a, "tokenId"), prompt: s(a, "prompt"), replies: arr(a, "replies").iter().map(|r| Opt { id: s(r, "id"), label: s(r, "label") }).collect() }).collect();
    let waits: Vec<Wait> = if live { waits_of(run, now, &base) } else { vec![] };
    let fl = arr(run, "findings");
    let (fh, flines) = model::findings(fl, true);
    vx.findings = if fl.is_empty() { String::new() } else { fh };
    vx.crashed = live && st == "crashed";
    vx.crash = s(&run["crash"], "reason");
    vx.cap = if live && truthy(&run["capHit"]) { model::cap_text(&run["capHit"]) } else { String::new() };
    vx.hint = if panel { "Tap someone at work to read their transcript live; tap a finished step to see what happened there." } else { "Tap a step to see what happened there and open that person's transcript." }.into();

    let mut steps = vec![];
    if let Some(node) = nodes_by.get(sel) {
        let who = { let p = model::person_of(node); if p.is_empty() && s(node, "type") == "decision" { s(&node["decision"], "answeredBy") } else { p } };
        let person = arr(f, "roster").iter().find(|r| s(r, "id") == who && !who.is_empty()).cloned();
        let chat_id = if live { if who == "manager" { s(run, "managerChatId") } else if !who.is_empty() { s(&employees[who.as_str()], "chatId") } else { String::new() } } else { String::new() };
        let visits: Vec<&Value> = history.iter().filter(|h| s(h, "type") == "handoff" && s(h, "nodeId") == sel).collect();
        let toks: Vec<&Value> = tokens.iter().filter(|t| s(t, "nodeId") == sel).collect();
        let retired = !who.is_empty() && if live { b(&employees[who.as_str()], "retired") } else { b(&rec["run"]["retired"], &who) };
        vx.sel = sel.to_string();
        vx.sel_title = { let l = s(node, "label"); if !l.is_empty() { l } else if let Some(p) = &person { s(p, "name") } else { s(node, "type") } };
        vx.who = person.as_ref().map(|p| { let m = s(p, "model"); format!("{}{}", s(p, "name"), if m.is_empty() { String::new() } else { format!(" · {m}") }) }).unwrap_or_default();
        let now_text = if let Some(t) = toks.first() { match s(t, "status").as_str() { "work" => "working on it", "answer" => "deciding", "ask" => "waiting for your reply", "join" => "waiting for the other branches", "wait" => "waiting", "finished" => "finished", _ => "" }.to_string() } else if !visits.is_empty() { "done".into() } else { "not reached yet".into() };
        vx.now = format!("{now_text}{}", if retired { " · session stopped for now" } else { "" });
        vx.visits = if visits.is_empty() { String::new() } else { visits.len().to_string() };
        vx.open_chat = if chat_id.is_empty() { String::new() } else if who == "manager" { "Open the Manager's transcript".into() } else { "Open their transcript".into() };
        vx.no_chat = if chat_id.is_empty() && person.is_some() && live { if retired { "Their session is stopped for now; it comes back with its memory when the work needs them." } else { "No live session right now." }.to_string() } else { String::new() };
        let nv = visits.len();
        for (i, h) in visits.iter().rev().enumerate() {
            let by = s(h, "by");
            let answers = h.get("answers").and_then(|a| a.as_object()).filter(|a| !a.is_empty()).map(|a| format!("Answers: {}", a.iter().map(|(k2, v2)| {
                let label = nodes_by.get(k2).map(|x| s(x, "label")).filter(|x| !x.is_empty()).unwrap_or_else(|| k2.clone());
                let val = match v2 { Value::Array(l) => if l.is_empty() { "none".into() } else { l.iter().map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).collect::<Vec<_>>().join(", ") }, Value::String(x) => x.clone(), o => o.to_string() };
                format!("{label}: {val}")
            }).collect::<Vec<_>>().join(" · "))).unwrap_or_default();
            steps.push(StepRow { who: format!("{}{}", names.get(&by).cloned().unwrap_or(by.clone()), if b(h, "finish") { " · finished" } else { "" }), num: format!("#{}", nv - i),
                summary: s(h, "summary"), answers, skills: arr(h, "skills").iter().filter_map(|x| x.as_str()).map(|k2| format!("/{}", k2.rsplit(':').next().unwrap_or(k2))).collect::<Vec<_>>().join("  ") });
        }
    }

    let mut timeline: Vec<Tl> = events.iter().enumerate().filter_map(|(i, ev)| model::describe(ev, names, nodes_by).map(|l| Tl {
        i: (i + 1) as i32, time: model::time_of(&s(ev, "at")), k: l.k, main: l.main, sub: l.sub, on: i + 1 == nn, future: i + 1 > nn,
    })).collect();
    timeline.reverse();
    vx.events = events.len() as i32;
    vx.at = nn as i32;
    vx.at_label = if live { "Live".into() } else { format!("{} · {nn}/{}", events.get(nn.saturating_sub(1)).map(|e| model::time_of(&s(e, "at"))).unwrap_or_default(), events.len()) };
    (kv, asks, waits, flines, steps, timeline, working, vx)
}

// ─── The canvas ───

pub fn size(cx: &mut App, w: f32, h: f32) {
    // A new size: fit again unless the person moved the view by hand.
    let refit = with(cx, |v| { let changed = (v.cw - w).abs() > 0.5 || (v.ch - h).abs() > 0.5; v.cw = w; v.ch = h; if changed && (v.follow || !v.moved) { v.fitted = false; v.active_key = "\u{1}".into(); } !v.fitted });
    if refit && with(cx, |v| v.open && v.run.is_some()) { build(cx); }
}
fn push_view(cx: &mut App) { with(cx, |v| { v.ui.zoom = v.z; v.ui.pan_x = v.px; v.ui.pan_y = v.py; v.ui.follow = v.follow; }); }
pub fn pan(cx: &mut App, dx: f32, dy: f32) { with(cx, |v| { v.px += dx; v.py += dy; v.follow = false; v.moved = true; }); push_view(cx); }
pub fn zoom_at(cx: &mut App, d: f32, mx: f32, my: f32) {
    with(cx, |v| {
        let z2 = (v.z * (d * 0.0025).exp()).clamp(0.15, 2.0);
        v.px = mx - (mx - v.px) * z2 / v.z; v.py = my - (my - v.py) * z2 / v.z; v.z = z2; v.follow = false; v.moved = true;
    });
    push_view(cx);
}
pub fn control(cx: &mut App, k: &str) {
    with(cx, |v| {
        let (cx, cy) = (v.cw / 2.0, v.ch / 2.0);
        let z2 = match k { "zoom-in" => (v.z * 1.2).min(2.0), "zoom-out" => (v.z / 1.2).max(0.15), _ => v.z };
        if k == "fit" {
            if let Some(bx) = model::union(&v.rects.iter().map(|r| r.1).collect::<Vec<_>>()) { let (z, px, py) = model::fit(bx, v.cw, v.ch, 0.15, 0.15, 1.0); v.z = z; v.px = px; v.py = py; }
        } else { v.px = cx - (cx - v.px) * z2 / v.z; v.py = cy - (cy - v.py) * z2 / v.z; v.z = z2; }
    });
    push_view(cx);
}
pub fn mini(cx: &mut App, fx: f32, fy: f32) { with(cx, |v| { v.px = v.cw / 2.0 - fx * v.z; v.py = v.ch / 2.0 - fy * v.z; v.follow = false; }); push_view(cx); }

/// A step clicked: in the panel, someone at work opens their transcript; otherwise the step's pane.
pub fn node_click(id: &str, cx: &mut App) {
    let (run, panel, live) = with(cx, |v| (v.run.clone().unwrap_or(Value::Null), v.panel, v.at.is_none()));
    let f = &run["snapshot"];
    let node = arr(f, "nodes").iter().find(|x| s(x, "id") == id).cloned().unwrap_or(Value::Null);
    let who = model::person_of(&node);
    let employees = &run["employees"];
    let tok_chat = arr(&run, "tokens").iter().find(|t| s(t, "nodeId") == id && { let x = s(t, "status"); x == "work" || x == "answer" }).map(|t| s(&employees[s(t, "employeeId")], "chatId")).unwrap_or_default();
    let chat = if !tok_chat.is_empty() { tok_chat } else if live && !who.is_empty() { if who == "manager" { s(&run, "managerChatId") } else { s(&employees[who.as_str()], "chatId") } } else { String::new() };
    if panel && !chat.is_empty() {
        let name = names_of(f).get(&who).cloned().unwrap_or_else(|| "Chat".into());
        open_person(&chat, &name, cx);
        return;
    }
    with(cx, |v| { v.sel = id.to_string(); v.tab = "run".into(); if v.panel { v.drawer = true; } });
    build(cx);
}
pub fn pane_click(cx: &mut App) { if with(cx, |v| !v.sel.is_empty()) { with(cx, |v| v.sel.clear()); build(cx); } }

// ─── People ───

/// The panel's transcript over the graph, on this chat (the driver's).
pub fn open_transcript_here(chat: &str, name: &str, cx: &mut App) {
    with(cx, |v| v.chat = Some((chat.to_string(), name.to_string())));
    build(cx);
}

/// A person's transcript: inside the panel, or the host's sheet.
fn open_person(chat: &str, name: &str, cx: &mut App) {
    if chat.is_empty() { return; }
    // (A chat that is a pane of the workbench, the Manager's, is that pane:
    // the chat area's views are made once per chat.)
    if with(cx, |v| v.panel) && !in_workbench(chat, cx) {
        with(cx, |v| v.chat = Some((chat.to_string(), name.to_string())));
        build(cx);
        return;
    }
    open_chat(chat, name, cx);
}

fn in_workbench(chat: &str, cx: &mut App) -> bool {
    crundi_shell::stores(cx).projects.read(cx).terminals.iter().any(|t| s(t, "id") == chat && s(t, "kind") == "ui")
}

/// openChat(): the Manager's chat is in the workbench; anyone else's opens as a sheet.
pub fn open_chat(chat: &str, name: &str, cx: &mut App) {
    if chat.is_empty() { return; }
    if in_workbench(chat, cx) {
        // The workbench, on that chat's pane.
        crate::util::show_tab(cx, "workbench");
        let key = format!("live:{chat}");
        cx.defer(move |cx| if let Some(wb) = crundi_workbench::workbench(cx) { wb.update(cx, |w, cx| w.focus_key(&key, cx)); });
        return;
    }
    fwith(cx, |s2| { s2.chat = Some((chat.to_string(), name.to_string())); s2.chat_name = if name.is_empty() { "Chat".into() } else { name.to_string() }; s2.chat_open = true; });
    super::bump(cx);
}

// ─── A person's live chat (the web's /chat/<id> page in a frame) ───

/// A transcript on show: the chat's own store (subscribed by this area and
/// fed from the live events, `route_chat`), the chat area's log and box over
/// it, and the page's header.
pub struct ChatView {
    pub id: String,
    pub pane: crundi_chat::pane::ChatPane,
    head: Entity<ChatHead>,
    /// The chat area held this chat too when it was made (it unsubscribes
    /// the chat when it lets go of it; then this area subscribes again).
    shared: bool,
}

/// The chats a transcript shows now: the panel's (over the graph), the tab's sheet.
fn wanted_chats(fx: &Fx) -> Vec<String> {
    let mut out = vec![];
    if fx.v.open && fx.v.panel && let Some((id, _)) = &fx.v.chat { out.push(id.clone()); }
    if fx.f.chat_open && let Some((id, _)) = &fx.f.chat && !out.contains(id) { out.push(id.clone()); }
    out
}

/// Transcripts no longer on show let go of their chat (from a handler: the
/// store's `bump`). Unsubscribed unless the chat area holds it too.
pub(crate) fn release_chats(cx: &mut App) {
    let gone: Vec<String> = super::with_fx(cx, |fx| {
        let want = wanted_chats(fx);
        let gone: Vec<String> = fx.f.chats.iter().filter(|c| !want.contains(&c.id)).map(|c| c.id.clone()).collect();
        fx.f.chats.retain(|c| want.contains(&c.id));
        gone
    });
    for id in gone { if crundi_chat::find(cx, &id).is_none() { crundi_shell::unsubscribe(cx, &id); } }
}

/// The frame's messages for the chats on show (ui-history, ui-event), in one
/// update per chat; a chat the chat area let go of is subscribed again.
pub(crate) fn route_chat(batch: &[crundi_core::live::Event], cx: &mut App) {
    let stores: Vec<(String, Entity<crundi_chat::store::ChatStore>)> = super::with_fx(cx, |fx| fx.f.chats.iter().map(|c| (c.id.clone(), c.pane.store.clone())).collect());
    if stores.is_empty() { return; }
    for (id, st) in &stores {
        let msgs: Vec<Value> = batch.iter().filter_map(|e| match e {
            crundi_core::live::Event::Chat(m) if m.get("id").and_then(|i| i.as_str()).or_else(|| m.pointer("/session/id").and_then(|i| i.as_str())) == Some(id.as_str()) => Some(m.clone()),
            _ => None,
        }).collect();
        if !msgs.is_empty() { st.update(cx, |s2, cx| s2.apply_batch(&msgs, cx)); }
    }
    // The chat area unsubscribes a chat it drops (its pane closed): ours
    // stays subscribed.
    if batch.iter().any(|e| matches!(e, crundi_core::live::Event::State(_))) {
        let gone: Vec<String> = stores.iter().map(|(id, _)| id.clone()).filter(|id| crundi_chat::find(cx, id).is_none()).collect();
        for id in gone {
            let was_shared = super::with_fx(cx, |fx| fx.f.chats.iter_mut().find(|c| c.id == id).map(|c| std::mem::replace(&mut c.shared, false)).unwrap_or(false));
            if was_shared { crundi_shell::subscribe(cx, &id); }
        }
    }
}

/// The chat's view for a transcript, made (and subscribed) the first time.
fn chat_of(fx: &mut Fx, id: &str, back: &'static str, project: &str, window: &mut Window, cx: &mut Context<Fx>) -> (crundi_chat::pane::ChatPane, Entity<ChatHead>) {
    if let Some(c) = fx.f.chats.iter().find(|c| c.id == id) {
        let (pane, head) = (c.pane.clone(), c.head.clone());
        head.update(cx, |h, cx| if h.back != back || h.project != project { h.back = back; h.project = project.to_string(); cx.notify(); });
        return (pane, head);
    }
    let store = cx.new(|cx| crundi_chat::store::ChatStore::new(id, cx));
    let mut pane = crundi_chat::pane::ChatPane::new(store.clone(), window, cx);
    // Mini lives in the workbench's chats, not here.
    pane.mini = None;
    crundi_chat::mini::detach(id, cx);
    let meta = store.read(cx).meta.clone();
    let head = cx.new(|cx| ChatHead { meta: meta.clone(), back, project: project.to_string(), _sub: cx.observe(&meta, |_, _, cx| cx.notify()) });
    // The server replays the conversation on subscribe (ui-history).
    crundi_shell::subscribe(cx, id);
    let shared = crundi_chat::find(cx, id).is_some();
    fx.f.chats.push(ChatView { id: id.to_string(), pane: pane.clone(), head: head.clone(), shared });
    (pane, head)
}

/// The chat page's header (#cp-head): back, the project's mark, the chat's
/// title and, under it, its state's dot, the project and the state's word.
pub struct ChatHead { meta: Entity<crundi_chat::store::ChatMeta>, back: &'static str, project: String, _sub: Subscription }
impl Render for ChatHead {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let m = &self.meta.read(cx).0;
        let title = if m.title.is_empty() { "Chat".to_string() } else { m.title.clone() };
        let word = match m.state.as_str() { "working" => "Working", "needs-input" => "Needs you", "waiting" => "Waiting", "idle" => "Ready", _ => "" };
        let sub = format!("{}{}{}", self.project, if !word.is_empty() && !self.project.is_empty() { " · " } else { "" }, word);
        let dot = match m.state.as_str() { "working" => T::amber(), "needs-input" => T::accent_hover(), "idle" => T::green(), _ => T::text_muted() };
        let back = self.back;
        div().flex().flex_none().items_center().gap(T::SP_10).px(T::SP_10).py(T::SP_8).bg(T::bg_secondary()).border_b_1().border_color(T::border_subtle())
            .child(div().id("fx-cp-back").flex_none().size(T::SP_32 + T::SP_6).rounded_full().flex().items_center().justify_center().cursor_pointer()
                .text_color(T::text_secondary()).active(|s| s.bg(T::bg_hover())).child(w::icon(IconName::ArrowLeft, T::SP_20))
                .on_click(move |_, _, cx| fact(back, "", "", cx)))
            .child(div().flex_none().size(T::SP_32 + T::SP_6).rounded(T::R_BUBBLE).bg(T::bg_tertiary()).flex().items_center().justify_center()
                .text_size(T::TEXT_15).font_weight(T::W_BOLD).text_color(T::white()).child(crundi_core::head::project_initials(&self.project)))
            .child(div().flex().flex_col().flex_1().min_w_0()
                .child(div().truncate().text_size(T::TEXT_15).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(title))
                .child(div().flex().items_center().gap(T::SP_6).min_w_0().text_size(T::TEXT_12).text_color(T::text_secondary())
                    .child(div().flex_none().size(px(7.)).rounded_full().bg(dot))
                    .child(div().truncate().child(sub))))
    }
}

// ─── Actions ───

fn run_act(body: Value, cx: &mut App) {
    let id = with(cx, |v| { v.busy = true; v.err.clear(); v.pause_menu = false; v.id.clone() });
    build(cx);
    let mut body = body;
    body["id"] = json!(id);
    parts::post(cx, "/api/factory-runs", body, |r, cx| {
        with(cx, |v| { v.busy = false; if !b(&r, "ok") { v.err = parts::err_text(&r, "That did not work."); } else { v.note.clear(); v.ask_note.clear(); } });
        build(cx);
        fetch(cx);
        // A stop or a finish changes what the panel shows.
        if is_panel(cx) { super::panel_load(cx); }
    });
}

pub fn act(a: &str, x: &str, y: &str, cx: &mut App) {
    match a {
        "v-back" => super::close_sub(cx),
        "v-pause" => {
            let opts = with(cx, |v| v.pause_opts);
            if opts { with(cx, |v| v.pause_menu = !v.pause_menu); build(cx); } else { run_act(json!({ "action": "pause" }), cx); }
        }
        "v-pause-reset" => { with(cx, |v| v.pause_reset = !v.pause_reset); build(cx); }
        "v-pause-go" => { let r = with(cx, |v| v.pause_reset); run_act(json!({ "action": "pause", "mode": x, "resumeOnReset": r }), cx); }
        "v-resume" => run_act(json!({ "action": "resume" }), cx),
        "v-stop" => parts::confirm(cx, "Stop", "Stop this factory? Its team stops; the worktree is kept.", "Stop", true, |ok, cx| { if ok { run_act(json!({ "action": "stop" }), cx); } }),
        "v-recover" => { let note = with(cx, |v| v.note.clone()); run_act(json!({ "action": "recover", "note": note }), cx); }
        "v-ask" => { let note = with(cx, |v| v.ask_note.clone()); run_act(json!({ "action": "ask", "tokenId": x, "replyId": y, "note": note }), cx); }
        "v-end-wait" => run_act(json!({ "action": "endWait", "tokenId": x, "outcome": y }), cx),
        "v-limits-open" => { with(cx, |v| { v.limits_open = !v.limits_open; if v.limits_open { v.limits = v.run.as_ref().map(|r| r["limits"].clone()).unwrap_or(Value::Null); } }); build(cx); }
        "v-limits" => { let l = with(cx, |v| { v.limits_open = false; v.limits.clone() }); run_act(json!({ "action": "limits", "limits": l }), cx); }
        "v-findings" => with(cx, |v| v.findings_open = !v.findings_open),
        "v-needs" => {
            let run = with(cx, |v| v.run.clone().unwrap_or(Value::Null));
            let st = s(&run, "status");
            if st == "crashed" { run_act(json!({ "action": "recover", "note": "" }), cx); }
            else if st == "finishing" { open_person(&s(&run, "managerChatId"), "Manager", cx); }
            else if truthy(&run["capHit"]) { run_act(json!({ "action": "resume" }), cx); }
            else if !arr(&run, "asks").is_empty() { with(cx, |v| { v.sel.clear(); v.tab = "run".into(); v.drawer = true; }); build(cx); }
            else if s(&run, "waitingOn") == "manager" { open_person(&s(&run, "managerChatId"), "Manager", cx); }
            else if truthy(&run["paused"]) { run_act(json!({ "action": "resume" }), cx); }
        }
        "v-manager-chat" => { let c = with(cx, |v| v.run.as_ref().map(|r| s(r, "managerChatId")).unwrap_or_default()); open_chat(&c, "Manager", cx); }
        "v-step-chat" => {
            let (run, sel) = with(cx, |v| (v.run.clone().unwrap_or(Value::Null), v.sel.clone()));
            let f = &run["snapshot"];
            let node = arr(f, "nodes").iter().find(|n2| s(n2, "id") == sel).cloned().unwrap_or(Value::Null);
            let who = { let p = model::person_of(&node); if p.is_empty() { s(&node["decision"], "answeredBy") } else { p } };
            let chat = if who == "manager" { s(&run, "managerChatId") } else { s(&run["employees"][who.as_str()], "chatId") };
            let name = names_of(f).get(&who).cloned().unwrap_or_else(|| "Chat".into());
            open_person(&chat, &name, cx);
        }
        "v-person" => open_person(x, y, cx),
        "v-chat-close" => { with(cx, |v| v.chat = None); build(cx); }
        "v-follow" => { with(cx, |v| { v.follow = !v.follow; if v.follow { v.active_key = "\u{1}".into(); } }); build(cx); }
        "v-drawer" => { with(cx, |v| v.drawer = !v.drawer); build(cx); }
        "v-tab" => { with(cx, |v| v.tab = x.to_string()); build(cx); }
        "v-unselect" => { with(cx, |v| v.sel.clear()); build(cx); }
        "v-at" => {
            let total = with(cx, |v| v.events.len());
            let n2: usize = x.parse().unwrap_or(total);
            with(cx, |v| v.at = if n2 >= total { None } else { Some(n2.max(1)) });
            build(cx);
        }
        "v-live" => { with(cx, |v| v.at = None); build(cx); }
        // The web copies without a word.
        "v-copy-curl" => cx.write_to_clipboard(ClipboardItem::new_string(format!("curl -X POST '{y}'"))),
        _ => {}
    }
}

pub fn edit(cx: &mut App, form: &str, k: &str, v: &str) {
    match form {
        "limits" => { let n2: i64 = v.trim().parse().unwrap_or(0); with(cx, |x| { if !x.limits.is_object() { x.limits = json!({}); } x.limits[k] = json!(n2); }); }
        "viewer" if k == "note" => with(cx, |x| x.note = v.to_string()),
        "viewer" if k == "ask-note" => with(cx, |x| x.ask_note = v.to_string()),
        _ => {}
    }
}

pub fn props(cx: &mut App) -> Value {
    // The transcripts' conversations, as their stores hold them (rows, entries, state).
    let chats: Vec<Value> = super::with_fx(cx, |fx| fx.f.chats.iter().map(|c| (c.id.clone(), c.pane.store.clone())).collect::<Vec<_>>()).into_iter().map(|(id, st)| {
        let s2 = st.read(cx);
        let m = &s2.meta.read(cx).0;
        json!({ "id": id, "rows": s2.rows.len(), "batches": s2.batches, "title": m.title, "state": m.state })
    }).collect();
    let mut o = props_of(cx);
    o["transcript"]["stores"] = json!(chats);
    o
}

fn props_of(cx: &mut App) -> Value {
    super::with_fx(cx, |fx| {
        let v = &mut fx.v;
        let u = &v.ui;
        let mut o = json!({
            "state": u.state, "error": u.error, "panel": u.panel, "name": u.name, "live": u.live, "status": u.st, "stateLabel": u.st_label,
            "progress": u.progress, "follow": u.follow, "drawer": u.drawer, "chat": u.chat, "canPause": u.can_pause, "canResume": u.can_resume, "canStop": u.can_stop,
            "pauseMenu": u.pause_menu, "needs": { "kind": u.needs_kind, "text": u.needs_text, "act": u.needs_act }, "tab": u.tab, "sel": u.sel,
        });
        let more = json!({ "selTitle": u.sel_title, "now": u.now, "openChat": u.open_chat, "err": u.err, "events": u.events, "at": u.at, "atLabel": u.at_label,
            "zoom": v.z, "crashed": u.crashed, "hint": u.hint, "runId": v.id });
        if let (Value::Object(a), Value::Object(b)) = (&mut o, more) { a.extend(b); }
        let lists = json!({
            "nodes": v.nodes.iter().map(|n2| json!({ "id": n2.id, "kind": n2.kind, "title": n2.title, "body": n2.body, "active": n2.active, "visited": n2.visited, "token": n2.token, "doing": n2.doing, "x": n2.x, "y": n2.y, "h": n2.h, "outs": n2.outs.iter().map(|o| o.label.clone()).collect::<Vec<_>>() })).collect::<Vec<_>>(),
            "edges": v.edges.iter().map(|e| json!({ "id": e.id, "tone": e.tone, "label": e.label })).collect::<Vec<_>>(),
            "working": v.working.iter().map(|w| json!({ "name": w.name, "what": w.what, "chat": w.chat })).collect::<Vec<_>>(),
            "kv": v.kv.iter().map(|x| json!([x.k, x.v])).collect::<Vec<_>>(),
            "timeline": v.timeline.iter().map(|t| json!({ "i": t.i, "main": t.main, "sub": t.sub, "k": t.k })).collect::<Vec<_>>(),
            "asks": v.asks.iter().map(|a| a.prompt.clone()).collect::<Vec<_>>(),
            "waits": v.waits.iter().map(|w| w.head.clone()).collect::<Vec<_>>(),
            "transcript": (|f: &super::F| json!({ "open": f.chat_open || v.chat.is_some(), "name": v.chat.as_ref().map(|c| c.1.clone()).unwrap_or_else(|| f.chat_name.clone()),
                "chat": v.chat.as_ref().or(f.chat.as_ref()).map(|c| c.0.clone()).unwrap_or_default(), "views": f.chats.iter().map(|c| c.id.clone()).collect::<Vec<_>>() }))(&fx.f),
            "waitTimer": v.wait_timer.is_some(),
        });
        if let (Value::Object(a), Value::Object(b)) = (&mut o, lists) { a.extend(b); }
        o
    })
}

// ─── Drawing ───

fn calls() -> Rc<parts::CanvasCalls> {
    Rc::new(parts::CanvasCalls {
        size: Box::new(|w, h, cx| size(cx, w, h)),
        pan: Box::new(|dx, dy, cx| pan(cx, dx, dy)),
        zoom: Box::new(|d, x, y, cx| zoom_at(cx, d, x, y)),
        control: Box::new(|k, cx| control(cx, k)),
        mini: Box::new(|x, y, cx| mini(cx, x, y)),
        pane: Box::new(|cx| pane_click(cx)),
        node_down: Box::new(|_, _| {}),
        node_drag: Box::new(|_, _, _, _| {}),
        node_up: Box::new(|id, moved, cx| if !moved { node_click(id, cx) }),
        handle_drag: Box::new(|_, _, _, _, _| {}),
        handle_up: Box::new(|_, _, _, _, _| {}),
        edge: Box::new(|_, _| {}),
        key: Box::new(|_, _| {}),
    })
}

fn a(what: &'static str, x: String, y: String) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static { move |_, _, cx| fact(what, &x, &y, cx) }
fn a0(what: &'static str) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static { a(what, String::new(), String::new()) }

/// The graph (in the tab or the panel).
pub fn render(fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let (u, nodes, edges, working, z, px, py) = rv(fx, |v| (v.ui.clone(), v.nodes.clone(), v.edges.clone(), v.working.clone(), v.z, v.px, v.py));
    if u.state == "loading" || u.state.is_empty() { return div().p(T::SP_24).child(note("note", "Loading the run…")).into_any_element(); }
    if u.state == "error" {
        return div().flex().flex_col().p(T::SP_16).gap(T::SP_10).child(note("err", u.error.clone()))
            .when(u.back, |d| d.child(div().child(btn("v-back-err", "Back", None, "", true, false).on_click(a0("v-back"))))).into_any_element();
    }
    let key: &'static str = if u.panel { "fx-v-panel" } else { "fx-v-tab" };
    let width = parts::bounds_of(&fx.p, key).map(|b| b.size.width).unwrap_or(T::FACTORIES_NARROW);
    let compact = width < T::FACTORIES_COMPACT;
    let panel = u.panel;
    let chat_over = panel && u.chat;
    // .ff-top
    let top = div().flex().flex_none().items_center().gap(T::SP_6).px(T::SP_8).py(T::SP_6).min_h(T::CONTROL + T::SP_10).bg(T::bg_secondary()).border_b_1().border_color(T::border())
        .when(u.back, |d| d.child(w::icon_button("v-back", IconName::ArrowLeft, "Back").on_click(a0("v-back"))))
        .when(chat_over, |d| d.child(btn("v-graph", "Graph", Some(IconName::ArrowLeft), "primary", true, compact).tooltip("Back to the graph").on_click(a0("v-chat-close"))))
        .child(div().id("v-name").min_w(T::SP_48).truncate().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).tooltip(w::tooltip(u.name.clone())).child(u.name.clone()))
        .child(w::tag(u.st_label.clone(), if u.live { parts::tone(parts::state_tone(&u.st)) } else { "" }))
        .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_11).text_color(T::text_muted()).when(!compact, |d| d.child(u.progress.clone())))
        .when(panel && !u.chat, |d| d.child(btn("v-follow", "Follow", Some(IconName::Route), if u.follow { "primary" } else { "" }, true, compact).tooltip("Follow: keep the view on where people are working").on_click(a0("v-follow"))))
        .when(panel, |d| d.child(btn("v-drawer", "Details", Some(IconName::SlidersHorizontal), if u.drawer { "primary" } else { "" }, true, compact).tooltip("Details: the run and its timeline").on_click(a0("v-drawer"))))
        .when(u.can_pause, |d| d.child(btn("v-pause", "Pause", compact.then_some(IconName::Pause), "", !u.busy, compact).tooltip("Pause").on_click(a0("v-pause"))))
        .when(u.can_resume, |d| d.child(btn("v-resume", "Resume", compact.then_some(IconName::Play), "", !u.busy, compact).tooltip("Resume").on_click(a0("v-resume"))))
        .when(u.can_stop, |d| d.child(btn("v-stop", "Stop", compact.then_some(IconName::Square), "ghost", !u.busy, compact).tooltip("Stop").on_click(a0("v-stop"))));
    let mut col = div().relative().flex().flex_col().size_full().min_h_0().child(parts::probe(cx.entity(), key, |_, _| {})).child(top);
    if u.pause_menu {
        col = col.child(div().flex().flex_col().gap(T::SP_6).px(T::SP_12).py(T::SP_8).bg(T::bg_card()).border_b_1().border_color(T::border())
            .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("Pause: nobody new starts a step."))
            .child(div().flex().flex_wrap().items_center().gap(T::SP_8)
                .child(w::check("v-pause-reset", "Carry on by itself when usage resets", u.pause_reset, |_, _, cx| fact("v-pause-reset", "", "", cx)))
                .child(btn("v-pause-finish", "Let current steps finish", None, "", !u.busy, false).on_click(a("v-pause-go", "finish".into(), String::new())))
                .child(btn("v-pause-now", "Stop them now", None, "ghost", !u.busy, false).on_click(a("v-pause-go", "now".into(), String::new())))
                .child(w::icon_button("v-pause-x", IconName::X, "Cancel").on_click(a0("v-pause")))));
    }
    if !u.needs_kind.is_empty() {
        let fg = match u.needs_kind.as_str() { "bad" => T::error_text(), "you" => T::yellow(), _ => T::text_secondary() };
        col = col.child(div().flex().flex_none().items_center().gap(T::SP_8).p(T::SP_8)
            .bg(match u.needs_kind.as_str() { "bad" => T::red_dim(), "you" => T::yellow_dim(), _ => T::bg_card() })
            .child(div().text_color(fg).child(w::icon(IconName::TriangleAlert, T::ICON_MD)))
            .child(div().flex_1().min_w_0().text_size(T::TEXT_12).text_color(fg).child(u.needs_text.clone()))
            .when(!u.needs_act.is_empty(), |d| d.child(btn("v-needs", &u.needs_act, None, "primary", !u.busy, false).on_click(a0("v-needs")))));
    }
    if !working.is_empty() {
        let mut row = div().flex().flex_none().flex_wrap().gap(T::SP_6).p(T::SP_6);
        for (i, wk) in working.iter().enumerate() {
            let (chat, name) = (wk.chat.clone(), wk.name.clone());
            // .ff-worker: who, and what they are at; busy green, blocked yellow.
            let tone = if wk.blocked { w::PillTone::Reconnecting } else if wk.busy { w::PillTone::Connected } else { w::PillTone::Neutral };
            let tip = if wk.queued { "Waiting for a free seat: the run's limit on people at once".to_string() } else if chat.is_empty() { String::new() } else { format!("Open {}'s transcript", wk.name) };
            let pill = w::status_pill(("v-worker", i), None, format!("{} {}", wk.name, wk.what), tone).when(wk.queued, |d| d.opacity(T::FACTORIES_O_SOFT));
            let pill = w::tip(pill, tip);
            row = row.child(if chat.is_empty() { pill } else { pill.on_click(move |_, _, cx| fact("v-person", &chat, &name, cx)) });
        }
        col = col.child(row);
    }
    // The transcript over the graph (.ff-chatover): the graph under it is
    // not drawn (a streamed reply then redraws the panel without it).
    if chat_over {
        let id = rv(fx, |v| v.chat.as_ref().map(|c| c.0.clone()).unwrap_or_default());
        return col.child(div().relative().flex().flex_col().flex_1().min_h_0().bg(T::bg()).child(transcript_body(fx, &id, "v-chat-close", window, cx))).into_any_element();
    }
    let cv = parts::canvas_view(&mut fx.p, cx.entity(), parts::Canvas { which: "v", nodes, edges, zoom: z, pan_x: px, pan_y: py, editable: false, minimap: !panel, controls_top: false, bounds: (u.bx, u.by, u.bw, u.bh), conn: None, cb: calls() }, window, cx);
    let column = !panel && width < T::FACTORIES_NARROW;
    let mut bodyd = div().relative().flex().flex_1().min_h_0();
    if column {
        let h = parts::bounds_of(&fx.p, key).map(|b| b.size.height).unwrap_or(T::FACTORIES_NARROW) * T::FACTORIES_CANVAS_SHARE;
        bodyd = bodyd.flex_col().child(div().flex_none().h(h).child(cv)).child(w::rule()).child(div().flex().flex_col().flex_1().min_h_0().child(side_view(&u, fx, window, cx)));
    } else {
        bodyd = bodyd.child(div().flex().flex_1().min_w_0().child(cv));
        if !panel { bodyd = bodyd.child(div().flex().flex_col().flex_none().w(T::FACTORIES_RIGHT).border_l_1().border_color(T::border()).child(side_view(&u, fx, window, cx))); }
    }
    if panel && u.drawer {
        bodyd = bodyd.child(div().absolute().top_0().bottom_0().right_0().w((T::FACTORIES_RIGHT + T::SP_20).min(width * T::FACTORIES_DRAWER_SHARE))
            .flex().flex_col().border_l_1().border_color(T::border()).shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_scroll(|_, _, cx| cx.stop_propagation())
            .child(side_view(&u, fx, window, cx)));
    }
    col = col.child(bodyd);
    if !panel && u.events > 1 { col = col.child(replay(&u)); }
    col.into_any_element()
}

/// The right side (.ff-right): the tabs (Run / Step, Timeline) and the pane.
fn side_view(u: &Vx, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    // .ff-tabs: Run (or Step), Timeline.
    let segs = vec![
        w::SegTab { icon: Some(IconName::SlidersHorizontal), label: if u.sel.is_empty() { "Run".into() } else { "Step".into() }, ..Default::default() },
        w::SegTab { icon: Some(IconName::Route), label: "Timeline".into(), ..Default::default() },
    ];
    let tabs = div().flex().flex_none().p(T::SP_6).border_b_1().border_color(T::border())
        .child(w::seg_tabs("v-tab", segs, if u.tab == "timeline" { 1 } else { 0 }, |i, _, cx| fact("v-tab", if i == 1 { "timeline" } else { "run" }, "", cx)));
    let pane = div().id("v-pane").flex().flex_col().flex_1().min_h_0().overflow_y_scroll().p(T::SP_12).gap(T::SP_10);
    let pane = if u.tab == "timeline" { timeline_pane(fx, pane) } else if !u.sel.is_empty() { step_pane(fx, pane, u) } else { run_pane(pane, u, fx, window, cx) };
    div().flex().flex_col().size_full().min_h_0().bg(T::bg_secondary()).child(tabs).child(pane).into_any_element()
}

fn actor(icon: IconName, head: String) -> Div {
    div().flex().flex_col().gap(T::SP_8).p(T::SP_10).rounded(T::R_FIELD).border_1().border_color(T::border()).bg(T::bg_card())
        .child(div().flex().gap(T::SP_6).child(div().pt(T::SP_2).child(w::icon(icon, T::ICON_MD))).child(div().flex_1().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).child(head)))
}

fn run_pane(mut d: Stateful<Div>, u: &Vx, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> Stateful<Div> {
    let (kvs, asks, waits, findings, fopen, ask_note) = rv(fx, |v| (v.kv.clone(), v.asks.clone(), v.waits.clone(), v.findings.clone(), v.findings_open, v.ask_note.clone()));
    for x in &kvs { d = d.child(kv(&x.k, &x.v)); }
    if u.has_limits {
        d = d.child(w::disclosure("v-limits-open", format!("Usage limits: {}", u.limits_sum), u.limits_open).on_click(a0("v-limits-open")));
        if u.limits_open {
            let f = |key: &str, label: &str, val: &str, fx: &mut Fx, window: &mut Window, cx: &mut App| {
                let k = key.to_string();
                parts::field(label, "", "", parts::line(&mut fx.p, &format!("limits|{key}"), val, "", false, false, parts::Commit::Change, window, cx, move |v, cx| super::edit_field("limits", &k, &v, cx)))
            };
            d = d.child(div().flex().flex_col().gap(T::SP_8)
                .child(div().flex().gap(T::SP_8).child(f("pauseAt5h", "5-hour, pause at %", &u.l5h, fx, window, cx)).child(f("pauseAtWeek", "Weekly, pause at %", &u.lweek, fx, window, cx)))
                .child(div().flex().gap(T::SP_8).child(f("maxParallel", "People at once (0: any)", &u.lpar, fx, window, cx)).child(f("oneAtATimeAbove", "One at a time above %", &u.lone, fx, window, cx)))
                .child(div().child(btn("v-limits", "Apply to this run", None, "primary", !u.busy, false).on_click(a0("v-limits")))));
        }
    }
    if u.manager_talks { d = d.child(note("hint", "The Manager is talking to you in its chat.")); }
    if u.manager_chat { d = d.child(div().child(btn("v-manager-chat", "Open the Manager's chat", Some(IconName::Crown), "", true, false).on_click(a0("v-manager-chat")))); }
    for (i, q) in asks.iter().enumerate() {
        let mut c = actor(IconName::MessageCircleQuestionMark, q.prompt.clone());
        if u.live {
            c = c.child(parts::area(&mut fx.p, "viewer|ask-note", &ask_note, "Optional note", 2, false, window, cx, |v, cx| super::edit_field("viewer", "ask-note", &v, cx)));
            let mut row = div().flex().flex_wrap().gap(T::SP_6);
            for (j, r) in q.replies.iter().enumerate() { row = row.child(btn(("v-reply", i * 100 + j), &r.label, None, "", !u.busy, false).on_click(a("v-ask", q.token.clone(), r.id.clone()))); }
            c = c.child(row);
        }
        d = d.child(c);
    }
    for (i, wt) in waits.iter().enumerate() {
        let mut c = actor(IconName::Clock, wt.head.clone());
        if !wt.gives.is_empty() { c = c.child(note("hint", wt.gives.clone())); }
        if !wt.last.is_empty() { c = c.child(div().p(T::SP_6).rounded(T::R_CONTROL).bg(T::bg()).font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_secondary()).child(wt.last.clone())); }
        if !wt.url.is_empty() {
            c = c.child(div().flex().flex_col().gap(T::SP_4)
                .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("Signal URL (POST)"))
                .child(div().flex().items_center().gap(T::SP_6)
                    .child(div().flex_1().min_w_0().child(parts::line(&mut fx.p, &format!("viewer|url:{i}"), &wt.url, "", true, false, parts::Commit::Leave, window, cx, |_, _| {})))
                    .child(btn(("v-curl", i), "curl", Some(IconName::Copy), "", true, false).on_click(a("v-copy-curl", wt.token.clone(), wt.url.clone()))))
                .child(note("hint", "Body optional: {\"note\": \"…\"}, or {\"outcome\": \"failed\"} for Timed out.")));
        }
        c = c.child(div().flex().gap(T::SP_6)
            .child(btn(("v-endwait", i), "End the wait now", None, "", !u.busy, false).on_click(a("v-end-wait", wt.token.clone(), "done".into())))
            .when(wt.two, |d| d.child(btn(("v-timeout", i), "Treat as timed out", None, "ghost", !u.busy, false).on_click(a("v-end-wait", wt.token.clone(), "timeout".into())))));
        d = d.child(c);
    }
    if !u.findings.is_empty() {
        d = d.child(w::disclosure("v-findings", u.findings.clone(), fopen).on_click(a0("v-findings")));
        if fopen { let mut l = div().flex().flex_col().gap(T::SP_4); for f in &findings { l = l.child(note("hint", f.clone())); } d = d.child(l); }
    }
    if u.crashed {
        d = d.child(actor(IconName::TriangleAlert, format!("It stopped: {}", u.crash))
            .child(parts::area(&mut fx.p, "viewer|note", &u.note, "Anything the team should know (optional)", 2, false, window, cx, |v, cx| super::edit_field("viewer", "note", &v, cx)))
            .child(div().child(btn("v-recover", "Recover", None, "primary", !u.busy, false).on_click(a0("v-recover")))));
    }
    if !u.cap.is_empty() {
        d = d.child(actor(IconName::TriangleAlert, format!("It reached its limit ({}).", u.cap))
            .child(div().child(btn("v-carry", "Let it carry on", None, "primary", !u.busy, false).on_click(a0("v-resume")))));
    }
    if !u.err.is_empty() { d = d.child(parts::issue("v-err", "error", &u.err, "")); }
    d.child(note("hint", u.hint.clone()))
}

fn step_pane(fx: &mut Fx, mut d: Stateful<Div>, u: &Vx) -> Stateful<Div> {
    d = d.child(div().flex().items_center().gap(T::SP_8)
        .child(w::icon_button("v-unselect", IconName::ArrowLeft, "Back").on_click(a0("v-unselect")))
        .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).child(u.sel_title.clone())));
    if !u.who.is_empty() { d = d.child(kv("Who", &u.who)); }
    d = d.child(kv("Now", &u.now));
    if !u.visits.is_empty() { d = d.child(kv("Visits", &u.visits)); }
    if !u.open_chat.is_empty() { d = d.child(div().child(btn("v-step-chat", &u.open_chat, Some(IconName::MessageCircleQuestionMark), "primary", true, false).on_click(a0("v-step-chat")))); }
    if !u.no_chat.is_empty() { d = d.child(note("hint", u.no_chat.clone())); }
    for st in rv(fx, |v| v.steps.clone()) {
        d = d.child(div().flex().flex_col().gap(T::SP_4).p(T::SP_10).rounded(T::R_FIELD).border_1().border_color(T::border()).bg(T::bg_card())
            .child(div().flex().text_size(T::TEXT_11).text_color(T::text_muted()).child(div().flex_1().truncate().child(st.who.clone())).child(st.num.clone()))
            .child(div().text_size(T::TEXT_12).child(st.summary.clone()))
            .when(!st.answers.is_empty(), |d| d.child(note("hint", st.answers.clone())))
            .when(!st.skills.is_empty(), |d| d.child(div().text_size(T::TEXT_10).text_color(T::green()).child(st.skills.clone()))));
    }
    d
}

fn timeline_pane(fx: &mut Fx, mut d: Stateful<Div>) -> Stateful<Div> {
    for (j, t) in rv(fx, |v| v.timeline.clone()).into_iter().enumerate() {
        let dotc = match t.k.as_str() { "start" | "finish" | "merged" => T::green(), "escalate" => T::yellow(), "crash" => T::red(), "handoff" => T::accent(), _ => T::text_muted() };
        let fg = match t.k.as_str() { "escalate" | "crash" => T::yellow(), "merged" | "finish" => T::green(), _ => T::text() };
        let i = t.i.to_string();
        d = d.child(div().id(("v-tl", j)).flex().gap(T::SP_8).p(T::SP_6).rounded(T::R_CONTROL).cursor_pointer()
            .when(t.on, |d| d.bg(T::accent_dim())).when(!t.on, |d| d.hover(|d| d.bg(T::bg_hover())))
            .when(t.future, |d| d.opacity(T::FACTORIES_O_DIM))
            .child(div().flex_none().w(T::SP_40).font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_muted()).child(t.time.clone()))
            .child(div().pt(T::SP_4).child(w::dot(dotc)))
            .child(div().flex().flex_col().flex_1().min_w_0().gap(T::SP_2)
                .child(div().text_size(T::TEXT_12).text_color(fg).child(t.main.clone()))
                .when(!t.sub.is_empty(), |d| d.child(div().text_size(T::TEXT_11).text_color(T::text_muted()).line_clamp(2).text_ellipsis().child(t.sub.clone()))))
            .on_click(move |_, _, cx| fact("v-at", &i, "", cx)));
    }
    d
}

/// The replay bar (.ff-replay): step back, the track, step forward, where.
fn replay(u: &Vx) -> AnyElement {
    let (nn, total) = (u.at, u.events);
    div().flex().flex_none().items_center().gap(T::SP_8).px(T::SP_8).h(T::CONTROL + T::SP_8).bg(T::bg_secondary()).border_t_1().border_color(T::border())
        .child(w::disabled(w::icon_button("v-back1", IconName::Undo2, "Step back"), nn <= 1).on_click(a("v-at", (nn - 1).max(1).to_string(), String::new())))
        .child(w::slider("v-track", 1.0, total.max(1) as f32, 1.0, nn.max(1) as f32, false, |v, _, cx| fact("v-at", &(v.round() as i32).to_string(), "", cx)))
        .child(w::disabled(w::icon_button("v-fwd1", IconName::Redo2, "Step forward"), u.live).on_click(a("v-at", (nn + 1).to_string(), String::new())))
        .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(u.at_label.clone()))
        .when(!u.live, |d| d.child(btn("v-live", "Back to live", None, "", true, false).on_click(a0("v-live"))))
        .into_any_element()
}

/// A person's transcript: their live chat, as the web's chat page in a
/// frame (its header; the conversation as the workbench draws it, Markdown
/// and all, live; the message box). `back` is the header's back arrow's
/// action (the panel's "back to the graph", the sheet's close).
pub fn transcript_body(fx: &mut Fx, chat: &str, back: &'static str, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    if chat.is_empty() { return div().into_any_element(); }
    let project = { let p = fx.v.run.as_ref().map(|r| s(r, "project")).unwrap_or_default(); if p.is_empty() { fx.f.panel.project.clone() } else { p } };
    let (pane, head) = chat_of(fx, chat, back, &project, window, cx);
    // (#cp-head: 8 px above and below a 38 px button.)
    div().id("fx-transcript").flex().flex_col().flex_1().min_h_0().size_full()
        .child(head.cached(StyleRefinement::default().w_full().h(T::SP_32 + T::SP_6 + T::SP_16).flex_none()))
        .child(div().flex().flex_col().flex_1().min_h_0().child(pane.body(cx)))
        .into_any_element()
}
