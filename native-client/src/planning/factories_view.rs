//! Factories (4.8): the live graph of a run (tools/factory-flow/src/Viewer.jsx).
//! One at a time: the tab's (with Back, the minimap, the replay bar) or the
//! panel's (Follow, Details, a transcript over the graph): never both on
//! show. It reads the run (/api/factory-runs/get: the summary, the
//! factory's snapshot and the history) and its journal (/events, only what
//! is new), on the server's factoryRun event and every few seconds while on
//! show (a missed push must not leave it behind).
//!
//! The web folds the journal with the server's reducer to replay any
//! moment; this client draws the run from the server's record, and a past
//! moment from the history up to it (which steps were done, the step at
//! work then, which connections were taken). Known gap: the replayed
//! moment shows no waits or asks of that time.

use super::model::{self, s, n, b, arr, Rect};
use super::{g, get, post, ss, rc_of, with as fwith, err_text};
use crate::{kit, PlanningFactories, PlanningFxNode, PlanningFxEdge, PlanningFxOut, PlanningFxWorker, PlanningFxKv, PlanningFxAsk, PlanningFxOpt, PlanningFxWait, PlanningFxTl, PlanningFxStep, PlanningFxView};
use serde_json::{json, Value};
use slint::{SharedString, VecModel};
use std::{cell::RefCell, collections::{HashMap, HashSet}, rc::Rc};

pub struct Models {
    nodes: Rc<VecModel<PlanningFxNode>>,
    edges: Rc<VecModel<PlanningFxEdge>>,
    working: Rc<VecModel<PlanningFxWorker>>,
    kv: Rc<VecModel<PlanningFxKv>>,
    asks: Rc<VecModel<PlanningFxAsk>>,
    waits: Rc<VecModel<PlanningFxWait>>,
    findings: Rc<VecModel<SharedString>>,
    timeline: Rc<VecModel<PlanningFxTl>>,
    steps: Rc<VecModel<PlanningFxStep>>,
    chat: Rc<VecModel<PlanningFxKv>>,
}

impl Models {
    pub fn new(f: &PlanningFactories) -> Models {
        let m = Models { nodes: kit::model(), edges: kit::model(), working: kit::model(), kv: kit::model(), asks: kit::model(), waits: kit::model(), findings: kit::model(), timeline: kit::model(), steps: kit::model(), chat: kit::model() };
        f.set_v_nodes(kit::rc(&m.nodes)); f.set_v_edges(kit::rc(&m.edges)); f.set_v_working(kit::rc(&m.working)); f.set_v_kv(kit::rc(&m.kv));
        f.set_v_asks(kit::rc(&m.asks)); f.set_v_waits(kit::rc(&m.waits)); f.set_v_findings(kit::rc(&m.findings)); f.set_v_timeline(kit::rc(&m.timeline));
        f.set_v_steps(kit::rc(&m.steps)); f.set_chat_lines(kit::rc(&m.chat));
        m
    }
}

#[derive(Default)]
struct V {
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
    pause_menu: bool,
    pause_reset: bool,
    pause_opts: bool,
    limits_open: bool,
    limits: Value,
    z: f32, px: f32, py: f32, cw: f32, ch: f32,
    fitted: bool,
    active_key: String,
    counts: HashMap<String, i64>,
    pulse: HashSet<String>,
    rects: Vec<(String, Rect)>,
    gen: u64,
    state: String,
    error: String,
    fetching: bool,
    moved: bool,
}

thread_local! {
    static VS: RefCell<V> = RefCell::new(V { z: 1.0, tab: "run".into(), ..Default::default() });
    static TIMER: slint::Timer = slint::Timer::default();
}
fn with<R>(f: impl FnOnce(&mut V) -> R) -> R { VS.with(|s| f(&mut s.borrow_mut())) }

/// Every few seconds, while the graph is on show (ACTIVITY_MS).
pub fn start_timer() {
    TIMER.with(|t| t.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(3000), || { if showing() { fetch(); } }));
}

pub fn reset() { with(|v| *v = V { z: 1.0, tab: "run".into(), ..Default::default() }); }
pub fn run_id() -> String { with(|v| v.id.clone()) }
pub fn is_panel() -> bool { with(|v| v.open && v.panel) }

/// Whether the graph is on show where it lives (its tab, or the panel on the workbench).
pub fn showing() -> bool {
    let (open, panel) = with(|v| (v.open, v.panel));
    if !open { return false; }
    if panel { super::panel_visible() } else { super::tab_visible() && fwith(|s| s.tab.sub == "viewer") }
}

/// Show a run's graph (the tab's, or the panel's).
pub fn open(id: &str, panel: bool) {
    let same = with(|v| v.open && v.id == id && v.panel == panel);
    if !same {
        // The canvas reports its own size (the tab's and the panel's differ): fit after that.
        with(|v| { *v = V { open: true, id: id.to_string(), panel, z: 1.0, tab: "run".into(), follow: panel, state: "loading".into(), ..Default::default() }; });
        clear_models();
        render();
        super::caps(|c| { with(|v| v.pause_opts = c.iter().any(|x| x == "factory-pause-options")); });
    }
    fetch();
}

pub fn close() {
    with(|v| { v.open = false; v.chat = None; });
    g(|f| { let mut x = f.get_v(); x.chat = false; f.set_v(x); });
}

fn clear_models() {
    super::models(|m| {
        let v = &m.v;
        v.nodes.set_vec(vec![]); v.edges.set_vec(vec![]); v.working.set_vec(vec![]); v.kv.set_vec(vec![]); v.asks.set_vec(vec![]);
        v.waits.set_vec(vec![]); v.findings.set_vec(vec![]); v.timeline.set_vec(vec![]); v.steps.set_vec(vec![]);
    });
}

/// The journal (only what is new) and the run's record, then drawn.
pub fn fetch() {
    let (id, from, gen, busy) = with(|v| { let b = v.fetching; v.fetching = true; (v.id.clone(), v.events.len(), v.gen, b) });
    if id.is_empty() || busy { return; }
    let ep = format!("/api/factory-runs/events?id={}&from={from}", kit::enc(&id));
    let rp = format!("/api/factory-runs/get?id={}", kit::enc(&id));
    kit::call(move |api| {
        let e = api.get(&ep).unwrap_or_else(|e| json!({ "ok": false, "error": e.to_string() }));
        let r = api.get(&rp).unwrap_or_else(|e| json!({ "ok": false, "error": e.to_string() }));
        Ok(json!({ "e": e, "r": r }))
    }, move |res| {
        with(|v| v.fetching = false);
        let Ok(x) = res else { return };
        if with(|v| v.id != id || v.gen != gen || !v.open) { return; }
        let (e, r) = (&x["e"], &x["r"]);
        let mut again = false;
        with(|v| {
            if b(r, "ok") { v.run = Some(r["run"].clone()); v.state = "ok".into(); }
            else if v.run.is_none() { let er = err_text(r, ""); if !er.is_empty() { v.state = "error".into(); v.error = er; } }
            if b(e, "ok") {
                let evs = arr(e, "events");
                if !evs.is_empty() && n(e, "from") as usize == v.events.len() { v.events.extend(evs.iter().cloned()); }
                else if (n(e, "total") as usize) < v.events.len() { v.events.clear(); again = true; }
            }
        });
        render();
        if again { fetch(); }
    });
}

// ─── Drawing ───

fn names_of(f: &Value) -> HashMap<String, String> { arr(f, "roster").iter().map(|r| (s(r, "id"), s(r, "name"))).collect() }

/// The history's handoffs (and where each was), oldest first.
fn handoffs(history: &[Value]) -> Vec<&Value> { history.iter().filter(|h| s(h, "type") == "handoff").collect() }

/// How many of the journal's first `n` events were handoffs that moved the work.
fn handoffs_in(events: &[Value], n: usize) -> usize {
    events.iter().take(n).filter(|e| s(e, "type") == "handoff" && !b(&e["payload"], "escalate")).count()
}

/// Edge use, from the order the steps handed off in: a connection s→t counts
/// each time a handoff at s is followed by one at t (Start leads to the first).
fn edge_counts(f: &Value, seq: &[String], active: &[String]) -> HashMap<String, i64> {
    let mut pairs: HashMap<(String, String), i64> = HashMap::new();
    let start = arr(f, "nodes").iter().find(|x| s(x, "type") == "start").map(|x| s(x, "id"));
    let mut path: Vec<String> = start.into_iter().collect();
    path.extend(seq.iter().cloned());
    for w in path.windows(2) { *pairs.entry((w[0].clone(), w[1].clone())).or_default() += 1; }
    if let Some(last) = path.last() { for a in active { if a != last { *pairs.entry((last.clone(), a.clone())).or_default() += 1; } } }
    arr(f, "edges").iter().filter(|e| s(e, "kind") == "flow").map(|e| (s(e, "id"), pairs.get(&(s(e, "source"), s(e, "target"))).copied().unwrap_or(0))).collect()
}

pub fn render() {
    let st = with(|v| (v.state.clone(), v.error.clone(), v.panel));
    if st.0 != "ok" {
        g(|f| { let mut x = f.get_v(); x.state = ss(&st.0); x.error = ss(&st.1); x.panel = st.2; x.back = !st.2; f.set_v(x); });
        return;
    }
    let now = model::now_ms();
    let (run, events, at, sel, tab, panel, busy) = with(|v| (v.run.clone().unwrap_or(Value::Null), v.events.clone(), v.at, v.sel.clone(), v.tab.clone(), v.panel, v.busy));
    let f = run.get("snapshot").cloned().unwrap_or(json!({ "nodes": [], "edges": [], "roster": [] }));
    let history: Vec<Value> = arr(&run, "history").to_vec();
    let names = names_of(&f);
    let nodes_by: HashMap<String, Value> = arr(&f, "nodes").iter().map(|x| (s(x, "id"), x.clone())).collect();
    let total = events.len();
    let nn = at.unwrap_or(total).min(total);
    let live = at.is_none();
    let status = s(&run, "status");
    let quiet = ["crashed", "paused", "stopped", "finished"].contains(&status.as_str());

    // What is where: live, the run's tokens; a past moment, the step at work then.
    let hs = handoffs(&history);
    let k = if live { hs.len() } else { handoffs_in(&events, nn).min(hs.len()) };
    let done: Vec<&Value> = hs.iter().take(k).copied().collect();
    let tokens: Vec<Value> = if live { arr(&run, "tokens").to_vec() } else { hs.get(k).map(|h| vec![json!({ "id": "replay", "nodeId": s(h, "nodeId"), "status": "work" })]).unwrap_or_default() };
    let mut tokens_at: HashMap<String, Vec<Value>> = HashMap::new();
    for t in &tokens { if !s(t, "nodeId").is_empty() && s(t, "job").is_empty() { tokens_at.entry(s(t, "nodeId")).or_default().push(t.clone()); } }
    let visited: HashSet<String> = done.iter().map(|h| s(h, "nodeId")).collect();
    let mut skills_at: HashMap<String, Vec<String>> = HashMap::new();
    for h in &done { for k2 in arr(h, "skills") { if let Some(x) = k2.as_str() { let e = skills_at.entry(s(h, "nodeId")).or_default(); if !e.iter().any(|y| y == x) { e.push(x.to_string()); } } } }
    let employees = run.get("employees").cloned().unwrap_or(json!({}));

    // Nodes.
    let mut out_nodes: Vec<PlanningFxNode> = vec![];
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
        if live && !quiet {
            if let Some(tok) = toks.iter().find(|t| { let x = s(t, "status"); x == "work" || x == "answer" }) {
                let emp = &employees[s(tok, "employeeId")];
                let act = &emp["activity"];
                doing = true;
                doing_who = { let e = s(tok, "employee"); if e.is_empty() { names.get(&s(tok, "employeeId")).cloned().unwrap_or_default() } else { e } };
                doing_text = { let t = s(act, "text"); if !t.is_empty() { t } else if s(tok, "status") == "answer" { "Deciding".into() } else { "Starting".into() } };
                doing_busy = s(act, "state") == "working";
            } else if !who.is_empty() {
                // A job the Manager handed out: drawn on the person's own step.
                if let Some(job) = tokens.iter().find(|t| !s(t, "job").is_empty() && s(t, "employeeId") == who && s(t, "status") == "work") {
                    if !job_nodes.iter().any(|(j, _)| *j == s(job, "id")) {
                        let e = &employees[who.as_str()];
                        let at2 = s(&e["activity"], "text");
                        doing = true;
                        doing_who = { let x = s(job, "employee"); if x.is_empty() { names.get(&who).cloned().unwrap_or_default() } else { x } };
                        doing_text = if at2.is_empty() { format!("Job: {}", model::first(&s(job, "job"), 60)) } else { format!("Job: {at2}") };
                        doing_busy = s(&e["activity"], "state") == "working";
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
        out_nodes.push(PlanningFxNode {
            id: ss(&id), kind: ss(&ty), kind_label: ss(model::kind_label(&ty).to_uppercase()), title: ss(model::node_title(node, &f)), body: ss(&body), lines,
            x, y, w, h, sel: sel == id, bad: false, active: !toks.is_empty(), visited: visited.contains(&id) && toks.is_empty(),
            retired: !who.is_empty() && b(&employees[who.as_str()], "retired"), badge: 0, badge_level: "".into(), token: ss(token), token_kind: ss(token_kind),
            doing, doing_who: ss(doing_who), doing_text: ss(doing_text), doing_busy, skills: ss(skills),
            outs: rc_of(outs.iter().map(|h2| PlanningFxOut { id: ss(h2), label: ss(model::handle_label(node, h2)), finish: h2 == "finish" }).collect()),
            agent: model::is_agent(&ty), has_in: ty != "start",
        });
        outs_of.insert(id.clone(), outs);
    }

    // Connections: which were taken, the ones just taken pulse.
    let seq: Vec<String> = done.iter().map(|h| s(h, "nodeId")).collect();
    let active_ids: Vec<String> = tokens_at.keys().cloned().collect();
    let counts = edge_counts(&f, &seq, &active_ids);
    let finished = !run.get("outcome").map(|o| o.is_null()).unwrap_or(true) && live;
    let prev = with(|v| std::mem::replace(&mut v.counts, counts.clone()));
    let pulse: HashSet<String> = counts.iter().filter(|(k2, c)| **c > prev.get(*k2).copied().unwrap_or(0) && !prev.is_empty()).map(|(k2, _)| k2.clone()).collect();
    let lanes = model::lanes(&f);
    let rect_of: HashMap<String, Rect> = rects.iter().cloned().collect();
    let mut out_edges: Vec<PlanningFxEdge> = vec![];
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
        out_edges.push(PlanningFxEdge {
            id: ss(&id), path: ss(path), arrow: ss(model::arrow(tx, ty, note)), tone: ss(tone), sel: false, pulse: pulse.contains(&id) || (fin && finished && prev.is_empty()),
            label: ss(&label), icon: ss(if n(e, "maxRounds") > 0.0 && !note { "loop" } else if note { "note" } else { "" }), lx, ly,
        });
    }
    // A job the Manager handed out: a line from the Manager to the person.
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
                out_edges.push(PlanningFxEdge { id: ss(format!("job-{}", s(t, "id"))), path: ss(path), arrow: ss(model::arrow(tx, ty, false)), tone: "job".into(), sel: false, pulse: false, label: "Job".into(), icon: "".into(), lx, ly });
            }
        }
    }
    let bounds = model::union(&rects.iter().map(|r| r.1).collect::<Vec<_>>()).unwrap_or(Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 });
    with(|v| { v.rects = rects.clone(); v.pulse = pulse; });

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
    let (follow, fitted, old_key, cw, ch) = with(|v| (v.follow, v.fitted, v.active_key.clone(), v.cw, v.ch));
    if cw > 0.0 && (!fitted || (follow && active_key != old_key)) {
        let pick: Vec<Rect> = if follow && !active_key.is_empty() { active_key.split(',').filter_map(|id| rect_of.get(id).copied()).collect() } else { vec![] };
        let (z, px, py) = if !pick.is_empty() { model::fit(model::union(&pick).unwrap(), cw, ch, 0.45, 0.15, 0.95) } else { model::fit(bounds, cw, ch, 0.15, 0.15, 1.0) };
        with(|v| { v.z = z; v.px = px; v.py = py; v.fitted = true; v.active_key = active_key.clone(); });
    } else { with(|v| v.active_key = active_key.clone()); }

    // The run pane, the step pane, the timeline.
    let (kv, asks, waits, findings, steps, timeline, working, vx) = side(&run, &f, &history, &events, nn, live, &sel, &names, &nodes_by, &tokens, now, panel);
    let mut vx = vx;
    vx.tab = ss(&tab);
    vx.busy = busy;
    vx.bx = bounds.x; vx.by = bounds.y; vx.bw = bounds.w; vx.bh = bounds.h;
    let (z, px, py) = with(|v| (v.z, v.px, v.py));
    vx.zoom = z; vx.pan_x = px; vx.pan_y = py;
    super::models(|m| {
        let v = &m.v;
        kit::sync_rows(&v.nodes, out_nodes);
        kit::sync_rows(&v.edges, out_edges);
        kit::sync_rows(&v.kv, kv);
        kit::sync_rows(&v.asks, asks);
        kit::sync_rows(&v.waits, waits);
        kit::sync_rows(&v.findings, findings.into_iter().map(ss).collect());
        kit::sync_rows(&v.steps, steps);
        kit::sync_rows(&v.timeline, timeline);
        kit::sync_rows(&v.working, working);
    });
    g(|f| f.set_v(vx));
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn side(run: &Value, f: &Value, history: &[Value], events: &[Value], nn: usize, live: bool, sel: &str, names: &HashMap<String, String>, nodes_by: &HashMap<String, Value>, tokens: &[Value], now: f64, panel: bool)
    -> (Vec<PlanningFxKv>, Vec<PlanningFxAsk>, Vec<PlanningFxWait>, Vec<String>, Vec<PlanningFxStep>, Vec<PlanningFxTl>, Vec<PlanningFxWorker>, PlanningFxView) {
    let st = s(run, "status");
    let quiet = ["crashed", "paused", "stopped", "finished"].contains(&st.as_str());
    let employees = run.get("employees").cloned().unwrap_or(json!({}));
    let (follow, drawer, chat, err, note, pause_menu, pause_reset, pause_opts, limits_open, limits) = with(|v| (v.follow, v.drawer, v.chat.is_some(), v.err.clone(), v.note.clone(), v.pause_menu, v.pause_reset, v.pause_opts, v.limits_open, v.limits.clone()));
    let mut vx = PlanningFxView { state: "ok".into(), panel, back: !panel, name: ss(s(f, "name")), live, follow, drawer, chat, err: ss(err), note: ss(note), pause_menu: pause_menu && live && !kit::truthy(&run["paused"]), pause_reset, pause_opts, limits_open, ..Default::default() };
    vx.st = ss(&st);
    vx.st_label = ss(if live { model::status_word(&st) } else { "Replay".into() });
    vx.progress = ss(model::progress(run, now));
    if live {
        let paused = kit::truthy(&run["paused"]);
        vx.can_pause = (st == "running" || st == "waiting") && !paused;
        vx.can_resume = paused;
        vx.can_stop = ["running", "waiting", "paused", "crashed", "finishing"].contains(&st.as_str());
        // What needs the person, said once.
        let mgr_chat = s(run, "managerChatId");
        let asks = arr(run, "asks");
        let (k, text, act) = if st == "crashed" { ("bad", format!("It stopped: {}. Recover picks it up where it was.", { let r = s(&run["crash"], "reason"); if r.is_empty() { "it was interrupted".into() } else { r } }), "Recover".to_string()) }
            else if st == "finishing" { ("you", "Done. The result is in the Manager's chat: merge it, ask for changes, keep the branch or discard it.".into(), if mgr_chat.is_empty() { String::new() } else { "Open the result".into() }) }
            else if kit::truthy(&run["capHit"]) { ("you", format!("It reached its limit ({}).", model::cap_text(&run["capHit"])), "Let it carry on".into()) }
            else if !asks.is_empty() { ("you", format!("A question for you: {}", s(&asks[0], "prompt")), "Answer".into()) }
            else if s(run, "waitingOn") == "manager" { ("you", "The Manager is talking to you in its chat.".into(), if mgr_chat.is_empty() { String::new() } else { "Open its chat".into() }) }
            else if paused { ("info", model::paused_text(&run["paused"]), "Resume".into()) }
            else { ("", String::new(), String::new()) };
        vx.needs_kind = ss(k); vx.needs_text = ss(text); vx.needs_act = ss(act);
    }
    // Everyone at work right now.
    let mut working: Vec<PlanningFxWorker> = vec![];
    if live && !quiet {
        for t in tokens.iter().filter(|t| { let x = s(t, "status"); x == "work" || x == "answer" }) {
            let e = &employees[s(t, "employeeId")];
            let what = if b(e, "held") { "Held by the Manager".to_string() } else if !s(t, "job").is_empty() { format!("Job: {}", model::first(&s(t, "job"), 50)) } else { let a = s(&e["activity"], "text"); if !a.is_empty() { a } else if s(t, "status") == "answer" { "Deciding".into() } else { "Starting".into() } };
            let blocked = b(t, "blocked");
            working.push(PlanningFxWorker { id: ss(s(t, "id")), name: ss({ let x = s(t, "employee"); if x.is_empty() { names.get(&s(t, "employeeId")).cloned().unwrap_or_else(|| "Someone".into()) } else { x } }),
                what: ss(if blocked { "Needs permission".into() } else { what }), busy: s(&e["activity"], "state") == "working", blocked, chat: ss(s(e, "chatId")), queued: false });
        }
    }
    if live { for x in arr(run, "waitingForSeat") { working.push(PlanningFxWorker { id: ss(s(x, "tokenId")), name: ss(s(x, "employee")), what: "Waiting its turn".into(), busy: false, blocked: false, chat: "".into(), queued: true }); } }

    // The run pane.
    let mut kv = vec![PlanningFxKv { k: "Task".into(), v: ss(s(run, "task")) }, PlanningFxKv { k: "Handoffs".into(), v: ss((n(run, "handoffs") as i64).to_string()) }];
    if !s(run, "branch").is_empty() { kv.push(PlanningFxKv { k: "Branch".into(), v: ss(s(run, "branch")) }); }
    if live && run.get("liveSessions").map(|x| !x.is_null()).unwrap_or(false) { kv.push(PlanningFxKv { k: "Sessions".into(), v: ss(format!("{} live", n(run, "liveSessions") as i64)) }); }
    let u = &run["usage"];
    if n(u, "turns") > 0.0 { let c = n(u, "costUsd"); kv.push(PlanningFxKv { k: "Spent".into(), v: ss(format!("{}{} turns in {} chats", if c > 0.0 { format!("${c:.2} · ") } else { String::new() }, n(u, "turns") as i64, n(u, "chats") as i64)) }); }
    let lim = &run["limits"];
    if live && lim.is_object() {
        vx.has_limits = true;
        vx.limits_sum = ss(format!("Pause at {}% / {}% · {}{}", n(lim, "pauseAt5h") as i64, n(lim, "pauseAtWeek") as i64,
            if n(lim, "maxParallel") > 0.0 { format!("at most {} at once", n(lim, "maxParallel") as i64) } else { "no limit on people".into() },
            if n(lim, "oneAtATimeAbove") > 0.0 { format!(" · one at a time above {}%", n(lim, "oneAtATimeAbove") as i64) } else { String::new() }));
        let l = if limits.is_object() && limits_open { limits } else { lim.clone() };
        vx.l5h = ss((n(&l, "pauseAt5h") as i64).to_string()); vx.lweek = ss((n(&l, "pauseAtWeek") as i64).to_string());
        vx.lpar = ss((n(&l, "maxParallel") as i64).to_string()); vx.lone = ss((n(&l, "oneAtATimeAbove") as i64).to_string());
    }
    vx.manager_talks = live && s(run, "waitingOn") == "manager";
    vx.manager_chat = live && !s(run, "managerChatId").is_empty();
    let asks: Vec<PlanningFxAsk> = if live { arr(run, "asks").iter().map(|a| PlanningFxAsk { token: ss(s(a, "tokenId")), prompt: ss(s(a, "prompt")), replies: rc_of(arr(a, "replies").iter().map(|r| PlanningFxOpt { id: ss(s(r, "id")), label: ss(s(r, "label")) }).collect()) }).collect() } else { vec![] };
    let waits: Vec<PlanningFxWait> = if live { arr(run, "tokens").iter().filter(|t| t.get("wait").map(|w| w.is_object()).unwrap_or(false)).map(|t| {
        let w = &t["wait"];
        let two = { let k = s(w, "kind"); k == "command" || k == "signal" };
        let sp = s(w, "signalPath");
        PlanningFxWait { token: ss(s(t, "id")), head: ss(model::wait_head(t, now, false)), gives: ss(if two { format!("Gives up in {}.", model::left(&s(w, "timeoutAt"), now)) } else { String::new() }),
            last: ss(if s(w, "kind") == "command" { s(w, "last") } else { String::new() }), url: ss(if sp.is_empty() { String::new() } else { format!("{}{sp}", kit::base()) }), two }
    }).collect() } else { vec![] };
    let fl = arr(run, "findings");
    let (fh, flines) = model::findings(fl, true);
    vx.findings = ss(if fl.is_empty() { String::new() } else { fh });
    vx.crashed = live && st == "crashed";
    vx.crash = ss(s(&run["crash"], "reason"));
    vx.cap = ss(if live && kit::truthy(&run["capHit"]) { model::cap_text(&run["capHit"]) } else { String::new() });
    vx.hint = ss(if panel { "Tap someone at work to read their transcript live; tap a finished step to see what happened there." } else { "Tap a step to see what happened there and open that person's transcript." });

    // The step pane.
    let mut steps = vec![];
    if let Some(node) = nodes_by.get(sel) {
        let who = { let p = model::person_of(node); if p.is_empty() && s(node, "type") == "decision" { s(&node["decision"], "answeredBy") } else { p } };
        let person = arr(f, "roster").iter().find(|r| s(r, "id") == who && !who.is_empty()).cloned();
        let chat_id = if live { if who == "manager" { s(run, "managerChatId") } else if !who.is_empty() { s(&employees[who.as_str()], "chatId") } else { String::new() } } else { String::new() };
        let visits: Vec<&Value> = history.iter().filter(|h| s(h, "type") == "handoff" && s(h, "nodeId") == sel).collect();
        let toks: Vec<&Value> = tokens.iter().filter(|t| s(t, "nodeId") == sel).collect();
        let retired = !who.is_empty() && b(&employees[who.as_str()], "retired");
        vx.sel = ss(sel);
        vx.sel_title = ss({ let l = s(node, "label"); if !l.is_empty() { l } else if let Some(p) = &person { s(p, "name") } else { s(node, "type") } });
        vx.who = ss(person.as_ref().map(|p| { let m = s(p, "model"); format!("{}{}", s(p, "name"), if m.is_empty() { String::new() } else { format!(" · {m}") }) }).unwrap_or_default());
        let now_text = if let Some(t) = toks.first() { match s(t, "status").as_str() { "work" => "working on it", "answer" => "deciding", "ask" => "waiting for your reply", "join" => "waiting for the other branches", "wait" => "waiting", "finished" => "finished", _ => "" }.to_string() } else if !visits.is_empty() { "done".into() } else { "not reached yet".into() };
        vx.now = ss(format!("{now_text}{}", if retired { " · session stopped for now" } else { "" }));
        vx.visits = ss(if visits.is_empty() { String::new() } else { visits.len().to_string() });
        vx.open_chat = ss(if chat_id.is_empty() { String::new() } else if who == "manager" { "Open the Manager's transcript".into() } else { "Open their transcript".into() });
        vx.no_chat = ss(if chat_id.is_empty() && person.is_some() && live { if retired { "Their session is stopped for now; it comes back with its memory when the work needs them." } else { "No live session right now." }.to_string() } else { String::new() });
        let nv = visits.len();
        for (i, h) in visits.iter().rev().enumerate() {
            let by = s(h, "by");
            let answers = h.get("answers").and_then(|a| a.as_object()).filter(|a| !a.is_empty()).map(|a| format!("Answers: {}", a.iter().map(|(k2, v2)| {
                let label = nodes_by.get(k2).map(|x| s(x, "label")).filter(|x| !x.is_empty()).unwrap_or_else(|| k2.clone());
                let val = match v2 { Value::Array(l) => if l.is_empty() { "none".into() } else { l.iter().map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).collect::<Vec<_>>().join(", ") }, Value::String(x) => x.clone(), o => o.to_string() };
                format!("{label}: {val}")
            }).collect::<Vec<_>>().join(" · "))).unwrap_or_default();
            steps.push(PlanningFxStep { who: ss(format!("{}{}", names.get(&by).cloned().unwrap_or(by.clone()), if b(h, "finish") { " · finished" } else { "" })), num: ss(format!("#{}", nv - i)),
                summary: ss(s(h, "summary")), answers: ss(answers), skills: ss(arr(h, "skills").iter().filter_map(|x| x.as_str()).map(|k2| format!("/{}", k2.rsplit(':').next().unwrap_or(k2))).collect::<Vec<_>>().join("  ")) });
        }
    }

    // The timeline, newest first.
    let mut timeline: Vec<PlanningFxTl> = events.iter().enumerate().filter_map(|(i, ev)| model::describe(ev, names, nodes_by).map(|l| PlanningFxTl {
        i: (i + 1) as i32, time: ss(model::time_of(&s(ev, "at"))), k: ss(l.k), main: ss(l.main), sub: ss(l.sub), on: i + 1 == nn, future: i + 1 > nn,
    })).collect();
    timeline.reverse();
    vx.events = events.len() as i32;
    vx.at = nn as i32;
    vx.at_label = ss(if live { "Live".into() } else { format!("{} · {nn}/{}", events.get(nn.saturating_sub(1)).map(|e| model::time_of(&s(e, "at"))).unwrap_or_default(), events.len()) });
    (kv, asks, waits, flines, steps, timeline, working, vx)
}

// ─── The canvas ───

pub fn size(w: f32, h: f32) {
    // A new size: fit again unless the person moved the view by hand.
    let refit = with(|v| { let changed = (v.cw - w).abs() > 0.5 || (v.ch - h).abs() > 0.5; v.cw = w; v.ch = h; if changed && (v.follow || !v.moved) { v.fitted = false; v.active_key = "\u{1}".into(); } !v.fitted });
    if refit && with(|v| v.open && v.run.is_some()) { render(); }
}
fn push_view() {
    let (z, px, py) = with(|v| (v.z, v.px, v.py));
    g(|f| { let mut x = f.get_v(); x.zoom = z; x.pan_x = px; x.pan_y = py; f.set_v(x); });
}
pub fn pan(dx: f32, dy: f32) { with(|v| { v.px += dx; v.py += dy; v.follow = false; v.moved = true; }); g(|f| { let mut x = f.get_v(); x.follow = false; f.set_v(x); }); push_view(); }
pub fn zoom_at(d: f32, mx: f32, my: f32) {
    with(|v| {
        let z2 = (v.z * (d * 0.0025).exp()).clamp(0.15, 2.0);
        v.px = mx - (mx - v.px) * z2 / v.z; v.py = my - (my - v.py) * z2 / v.z; v.z = z2; v.follow = false; v.moved = true;
    });
    g(|f| { let mut x = f.get_v(); x.follow = false; f.set_v(x); });
    push_view();
}
pub fn control(k: &str) {
    with(|v| {
        let (cx, cy) = (v.cw / 2.0, v.ch / 2.0);
        let z2 = match k { "zoom-in" => (v.z * 1.2).min(2.0), "zoom-out" => (v.z / 1.2).max(0.15), _ => v.z };
        if k == "fit" {
            if let Some(bx) = model::union(&v.rects.iter().map(|r| r.1).collect::<Vec<_>>()) { let (z, px, py) = model::fit(bx, v.cw, v.ch, 0.15, 0.15, 1.0); v.z = z; v.px = px; v.py = py; }
        } else { v.px = cx - (cx - v.px) * z2 / v.z; v.py = cy - (cy - v.py) * z2 / v.z; v.z = z2; }
    });
    push_view();
}
pub fn mini(fx: f32, fy: f32) { with(|v| { v.px = v.cw / 2.0 - fx * v.z; v.py = v.ch / 2.0 - fy * v.z; v.follow = false; }); push_view(); }

/// A step clicked: in the panel, someone at work opens their transcript; otherwise the step's pane.
pub fn node_click(id: &str) {
    let (run, panel, live) = with(|v| (v.run.clone().unwrap_or(Value::Null), v.panel, v.at.is_none()));
    let f = &run["snapshot"];
    let node = arr(f, "nodes").iter().find(|x| s(x, "id") == id).cloned().unwrap_or(Value::Null);
    let who = model::person_of(&node);
    let employees = &run["employees"];
    let tok_chat = arr(&run, "tokens").iter().find(|t| s(t, "nodeId") == id && { let x = s(t, "status"); x == "work" || x == "answer" }).map(|t| s(&employees[s(t, "employeeId")], "chatId")).unwrap_or_default();
    let chat = if !tok_chat.is_empty() { tok_chat } else if live && !who.is_empty() { if who == "manager" { s(&run, "managerChatId") } else { s(&employees[who.as_str()], "chatId") } } else { String::new() };
    if panel && !chat.is_empty() {
        let name = names_of(f).get(&who).cloned().unwrap_or_else(|| "Chat".into());
        open_person(&chat, &name);
        return;
    }
    with(|v| { v.sel = id.to_string(); v.tab = "run".into(); if v.panel { v.drawer = true; } });
    render();
}
pub fn pane_click() { if with(|v| !v.sel.is_empty()) { with(|v| v.sel.clear()); render(); } }

// ─── People ───

/// A person's transcript: inside the panel, or the host's chat window.
fn open_person(chat: &str, name: &str) {
    if chat.is_empty() { return; }
    if with(|v| v.panel) {
        with(|v| v.chat = Some((chat.to_string(), name.to_string())));
        load_transcript(chat);
        render();
        return;
    }
    open_chat(chat, name);
}

/// openChat(): the Manager's chat is in the workbench; anyone else's opens as a sheet.
pub fn open_chat(chat: &str, name: &str) {
    if chat.is_empty() { return; }
    let in_workbench = kit::terminals().unwrap_or_default().iter().any(|t| s(t, "id") == chat && s(t, "kind") == "ui");
    if in_workbench {
        // Focusing that pane is the workspace's (requested); meanwhile the workbench.
        kit::show_tab("workbench");
        return;
    }
    fwith(|s2| s2.chat = Some((chat.to_string(), name.to_string())));
    g(|f| { f.set_chat_name(ss(if name.is_empty() { "Chat" } else { name })); f.set_chat_open(true); });
    super::dlg::sync_modal();
    load_transcript(chat);
}

pub fn load_transcript(chat: &str) {
    g(|f| f.set_chat_state("loading".into()));
    let chat2 = chat.to_string();
    get(format!("/api/ui-sessions/{}/history", kit::enc(chat)), move |r| {
        if !b(&r, "ok") { g(|f| f.set_chat_state("error".into())); return; }
        let lines: Vec<PlanningFxKv> = arr(&r["session"], "messages").iter().rev().take(300).collect::<Vec<_>>().into_iter().rev().filter_map(|m| {
            let kind = s(m, "kind");
            match kind.as_str() {
                "user" => Some(PlanningFxKv { k: "user".into(), v: ss(s(m, "text")) }),
                "assistant-text" | "result" | "error" => { let t = s(m, "text"); (!t.is_empty()).then(|| PlanningFxKv { k: "claude".into(), v: ss(t) }) }
                "tool" => Some(PlanningFxKv { k: "tool".into(), v: ss(format!("{} {}", s(m, "name"), model::first(&m.get("input").map(|i| { let c = s(i, "command"); if !c.is_empty() { c } else { let p = s(i, "file_path"); if !p.is_empty() { p } else { i.to_string() } } }).unwrap_or_default(), 160))) }),
                _ => None,
            }
        }).collect();
        super::models(|m| kit::sync_rows(&m.v.chat, lines));
        g(|f| f.set_chat_state("ok".into()));
        let _ = chat2;
    });
}

// ─── Actions ───

fn run_act(body: Value) {
    let id = with(|v| { v.busy = true; v.err.clear(); v.pause_menu = false; v.id.clone() });
    render();
    let mut body = body;
    body["id"] = json!(id);
    post("/api/factory-runs", body, |r| {
        with(|v| { v.busy = false; if !b(&r, "ok") { v.err = err_text(&r, "That did not work."); } else { v.note.clear(); } });
        if b(&r, "ok") { g(|f| f.set_v_ask_note("".into())); }
        render();
        fetch();
    });
}

pub fn act(a: &str, x: &str, y: &str) {
    match a {
        "v-back" => super::close_sub(),
        "v-pause" => {
            let opts = with(|v| v.pause_opts);
            if opts { with(|v| v.pause_menu = !v.pause_menu); render(); } else { run_act(json!({ "action": "pause" })); }
        }
        "v-pause-reset" => { with(|v| v.pause_reset = !v.pause_reset); render(); }
        "v-pause-go" => { let r = with(|v| v.pause_reset); run_act(json!({ "action": "pause", "mode": x, "resumeOnReset": r })); }
        "v-resume" => run_act(json!({ "action": "resume" })),
        "v-stop" => kit::confirm(kit::Confirm { title: "Stop", message: "Stop this factory? Its team stops; the worktree is kept.", ok: "Stop", danger: true }, |ok| { if ok { run_act(json!({ "action": "stop" })); } }),
        "v-recover" => { let note = with(|v| v.note.clone()); run_act(json!({ "action": "recover", "note": note })); }
        "v-ask" => { let note = kit::ui().map(|w| { use slint::ComponentHandle; w.global::<PlanningFactories>().get_v_ask_note().to_string() }).unwrap_or_default(); run_act(json!({ "action": "ask", "tokenId": x, "replyId": y, "note": note })); }
        "v-end-wait" => run_act(json!({ "action": "endWait", "tokenId": x, "outcome": y })),
        "v-limits-open" => { with(|v| { v.limits_open = !v.limits_open; if v.limits_open { v.limits = v.run.as_ref().map(|r| r["limits"].clone()).unwrap_or(Value::Null); } }); render(); }
        "v-limits" => { let l = with(|v| { v.limits_open = false; v.limits.clone() }); run_act(json!({ "action": "limits", "limits": l })); }
        "v-needs" => {
            let run = with(|v| v.run.clone().unwrap_or(Value::Null));
            let st = s(&run, "status");
            if st == "crashed" { run_act(json!({ "action": "recover", "note": "" })); }
            else if st == "finishing" { open_person(&s(&run, "managerChatId"), "Manager"); }
            else if kit::truthy(&run["capHit"]) { run_act(json!({ "action": "resume" })); }
            else if !arr(&run, "asks").is_empty() { with(|v| { v.sel.clear(); v.tab = "run".into(); v.drawer = true; }); render(); }
            else if s(&run, "waitingOn") == "manager" { open_person(&s(&run, "managerChatId"), "Manager"); }
            else if kit::truthy(&run["paused"]) { run_act(json!({ "action": "resume" })); }
        }
        "v-manager-chat" => { let c = with(|v| v.run.as_ref().map(|r| s(r, "managerChatId")).unwrap_or_default()); open_chat(&c, "Manager"); }
        "v-step-chat" => {
            let (run, sel) = with(|v| (v.run.clone().unwrap_or(Value::Null), v.sel.clone()));
            let f = &run["snapshot"];
            let node = arr(f, "nodes").iter().find(|n2| s(n2, "id") == sel).cloned().unwrap_or(Value::Null);
            let who = { let p = model::person_of(&node); if p.is_empty() { s(&node["decision"], "answeredBy") } else { p } };
            let chat = if who == "manager" { s(&run, "managerChatId") } else { s(&run["employees"][who.as_str()], "chatId") };
            let name = names_of(f).get(&who).cloned().unwrap_or_else(|| "Chat".into());
            open_person(&chat, &name);
        }
        "v-person" => open_person(x, y),
        "v-chat-close" => { with(|v| v.chat = None); render(); }
        "v-follow" => { with(|v| { v.follow = !v.follow; if v.follow { v.active_key = "\u{1}".into(); } }); render(); }
        "v-drawer" => { with(|v| v.drawer = !v.drawer); render(); }
        "v-tab" => { with(|v| v.tab = x.to_string()); render(); }
        "v-unselect" => { with(|v| v.sel.clear()); render(); }
        "v-at" => {
            let total = with(|v| v.events.len());
            let n2: usize = x.parse().unwrap_or(total);
            with(|v| v.at = if n2 >= total { None } else { Some(n2.max(1)) });
            render();
        }
        "v-live" => { with(|v| v.at = None); render(); }
        "v-copy-curl" => {
            let text = format!("curl -X POST '{y}'");
            match arboard::Clipboard::new().and_then(|mut c| c.set_text(text)) { Ok(_) => kit::toast("Copied."), Err(e) => kit::toast_error(&e.to_string()) }
        }
        _ => {}
    }
}

pub fn edit(form: &str, k: &str, v: &str) {
    match form {
        "limits" => { let n2: i64 = v.trim().parse().unwrap_or(0); with(|x| { if !x.limits.is_object() { x.limits = json!({}); } x.limits[k] = json!(n2); }); }
        "viewer" if k == "note" => with(|x| x.note = v.to_string()),
        _ => {}
    }
}

pub fn props(f: &PlanningFactories) -> Value {
    use slint::Model;
    let v = f.get_v();
    json!({
        "state": v.state.as_str(), "error": v.error.as_str(), "panel": v.panel, "name": v.name.as_str(), "live": v.live, "status": v.st.as_str(), "stateLabel": v.st_label.as_str(),
        "progress": v.progress.as_str(), "follow": v.follow, "drawer": v.drawer, "chat": v.chat, "canPause": v.can_pause, "canResume": v.can_resume, "canStop": v.can_stop,
        "pauseMenu": v.pause_menu, "needs": { "kind": v.needs_kind.as_str(), "text": v.needs_text.as_str(), "act": v.needs_act.as_str() }, "tab": v.tab.as_str(), "sel": v.sel.as_str(),
        "selTitle": v.sel_title.as_str(), "now": v.now.as_str(), "openChat": v.open_chat.as_str(), "err": v.err.as_str(), "events": v.events, "at": v.at, "atLabel": v.at_label.as_str(),
        "zoom": v.zoom, "crashed": v.crashed, "hint": v.hint.as_str(), "runId": run_id(),
        "nodes": f.get_v_nodes().iter().map(|n2| json!({ "id": n2.id.as_str(), "kind": n2.kind.as_str(), "title": n2.title.as_str(), "body": n2.body.as_str(), "active": n2.active, "visited": n2.visited, "token": n2.token.as_str(), "doing": n2.doing, "x": n2.x, "y": n2.y, "h": n2.h, "outs": n2.outs.iter().map(|o| o.label.to_string()).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "edges": f.get_v_edges().iter().map(|e| json!({ "id": e.id.as_str(), "tone": e.tone.as_str(), "label": e.label.as_str() })).collect::<Vec<_>>(),
        "working": f.get_v_working().iter().map(|w| json!({ "name": w.name.as_str(), "what": w.what.as_str(), "chat": w.chat.as_str() })).collect::<Vec<_>>(),
        "kv": f.get_v_kv().iter().map(|x| json!([x.k.as_str(), x.v.as_str()])).collect::<Vec<_>>(),
        "timeline": f.get_v_timeline().iter().map(|t| json!({ "i": t.i, "main": t.main.as_str(), "sub": t.sub.as_str(), "k": t.k.as_str() })).collect::<Vec<_>>(),
        "asks": f.get_v_asks().iter().map(|a| a.prompt.to_string()).collect::<Vec<_>>(),
        "waits": f.get_v_waits().iter().map(|w| w.head.to_string()).collect::<Vec<_>>(),
        "transcript": { "open": f.get_chat_open(), "name": f.get_chat_name().as_str(), "state": f.get_chat_state().as_str(), "lines": f.get_chat_lines().row_count() },
    })
}
