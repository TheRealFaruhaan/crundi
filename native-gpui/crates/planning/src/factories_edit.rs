//! Factories (4.7): the factory editor (tools/factory-flow/src/Editor.jsx,
//! Palette, Inspector, Roster, CrTemplate). The factory stays the source of
//! truth: every change edits its JSON (undo and redo keep copies, typing
//! in one field coalesces), the canvas is drawn from it, and it is saved
//! 1.2 s after the last change (Ctrl+S at once; Saved / Unsaved / Saving… /
//! Changed elsewhere with Reload / Not saved with Retry).
//!
//! Known gaps (as the Slint client): the web checks a factory in the browser
//! as it changes (the server's validateFactory, bundled); this client shows
//! the server's check, which comes back with each save. The Dry run walks the
//! flow with the server's engine, bundled in the web; this client has none.
//! Typing "/" in a persona does not offer the skills (the named skills still
//! show under the text, marked when not installed).

use super::model::{self, arr, b, n, s, Rect};
use super::parts::{self, btn, field, note, sub, Check, CrSec, Edge, Issue, Node, Opt, Out, Person};
use super::{act as fact, edit_field};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::{HashMap, HashSet}, rc::Rc};

/// The editor's top bar (PlanningFxEd).
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Ed {
    pub state: String, pub error: String, pub name: String, pub save: String, pub save_label: String, pub draft: bool,
    pub can_undo: bool, pub can_redo: bool, pub tab: String, pub issues_label: String, pub issues_bad: bool, pub toast: String,
    pub zoom: f32, pub pan_x: f32, pub pan_y: f32, pub bx: f32, pub by: f32, pub bw: f32, pub bh: f32,
    pub conn: bool, pub conn_path: String,
}

/// The right pane (PlanningFxInsp): what it edits.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Insp {
    pub mode: String, pub kind: String, pub kind_label: String, pub can_delete: bool, pub issues: Vec<Issue>,
    pub label: String, pub label_ph: String,
    pub who: Vec<String>, pub who_i: usize, pub has_person: bool,
    pub question: String, pub dkind_i: usize, pub options: Vec<Opt>, pub review: bool, pub answered: Vec<String>, pub answered_i: usize,
    pub prompt: String, pub replies: Vec<Opt>,
    pub wkind_i: usize, pub minutes: String, pub at: String, pub command: String, pub every: String, pub what: String, pub timeout: String,
    pub forks: Vec<String>, pub fork_i: usize,
    pub edge_desc: String, pub note: bool, pub note_label: String, pub rounds: String, pub require_note: bool,
    pub name: String, pub description: String, pub cr: Vec<CrSec>, pub cr_default: bool, pub cr_full: bool,
    pub mmode_i: usize, pub mmode_hint: String, pub tmode_i: usize, pub tmode_hint: String, pub bypass_warn: bool,
    pub presets: Vec<Check>, pub rules: String, pub manager_rules: bool, pub mrules_hint: String,
    pub gatekeeper: bool, pub gk_i: usize,
    pub secrets_on: bool, pub secrets: Vec<Check>, pub secrets_gone: Vec<String>,
    pub merge_i: usize, pub auto_recover: bool, pub setup: String, pub max_handoffs: String, pub max_minutes: String,
    pub tick: String, pub tick_hint: String, pub l5h: String, pub lweek: String, pub lim_hint: String, pub lpar: String, pub lone: String,
    pub people: Vec<Person>, pub personas: Vec<Opt>,
    pub manager: bool, pub persona: String, pub model_i: usize, pub effort_i: usize, pub memory_i: usize, pub memory_hint: String,
    pub can_edit: bool, pub pinned: String, pub p_presets: Vec<Check>, pub show_presets: bool, pub allow: String, pub deny: String, pub steps_hint: String,
    pub mentions_q: Vec<Check>, pub mentions_p: Vec<Check>, pub mentions_d: Vec<Check>,
    pub ok: bool, pub issues_title: String, pub issues_hint: String,
}

#[derive(Default)]
struct E {
    open: bool,
    id: String,
    f: Value,
    version: i64,
    validation: Value,
    save: String,
    dirty: bool,
    past: Vec<Value>,
    future: Vec<Value>,
    last_key: String,
    last_at: f64,
    sel: Option<(String, String)>,
    tab: String,
    roster_sel: Option<String>,
    roster_add_to: Option<String>,
    skills: Vec<(String, String, String)>,
    secrets: Option<Vec<Value>>,
    z: f32, px: f32, py: f32, cw: f32, ch: f32,
    fitted: bool,
    drag: Option<(String, f64, f64, Value, bool)>,
    conn: Option<String>,
    toast: String,
    toast_gen: u64,
    save_gen: u64,
    insp_key: String,
    state: String,
    error: String,
    rects: HashMap<String, Rect>,
    outs: HashMap<String, Vec<String>>,
    gn: u64,
    cr_open: bool,
    more_open: bool,
    // What it shows.
    ed: Ed,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    people: Vec<Opt>,
    insp: Option<Insp>,
}

thread_local! { static ES: RefCell<E> = RefCell::new(E { z: 1.0, ..Default::default() }); }
fn with<R>(f: impl FnOnce(&mut E) -> R) -> R { ES.with(|s| f(&mut s.borrow_mut())) }

fn data() -> Value { serde_json::from_str(super::data::DATA).unwrap_or(Value::Null) }
const EDIT_TOOLS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// Closing the editor: what is unsaved is saved first.
pub fn reset(cx: &mut App) {
    let (open, dirty) = with(|e| (e.open, e.dirty));
    if open && dirty { save_now(cx, |_, _| {}); }
    reset_quiet();
}
pub fn reset_quiet() { with(|e| { let (cw, ch) = (e.cw, e.ch); *e = E { z: 1.0, cw, ch, ..Default::default() }; }); }
pub fn zoom() -> f32 { with(|e| e.z) }

/// mountEditor(): load the factory, then the skills and the secrets.
pub fn open(id: &str, cx: &mut App) {
    let gn = with(|e| { let (cw, ch, g2) = (e.cw, e.ch, e.gn + 1); *e = E { open: true, id: id.to_string(), z: 1.0, cw, ch, tab: "edit".into(), save: "saved".into(), state: "loading".into(), gn: g2, ..Default::default() }; g2 });
    parts::forget("ed|");
    push_ed();
    parts::get(cx, format!("/api/factories/get?id={}", parts::enc(id)), move |j, _| {
        if with(|e| e.gn != gn) { return; }
        if !b(&j, "ok") {
            let er = parts::err_text(&j, "Could not open this factory.");
            with(|e| { e.state = "error".into(); e.error = er; });
            push_ed();
            return;
        }
        with(|e| { e.f = j["factory"].clone(); e.version = n(&j["factory"], "version") as i64; e.validation = j["validation"].clone(); e.state = "ok".into(); });
        render_model();
    });
    parts::get(cx, "/api/skills".into(), move |j, _| {
        if with(|e| e.gn != gn) { return; }
        let list: Vec<(String, String, String)> = arr(&j, "skills").iter().map(|x| (s(x, "name"), s(x, "description"), s(x, "scope"))).collect();
        with(|e| e.skills = list);
        render_model();
    });
    super::caps(cx, move |c, cx| {
        if !c.iter().any(|x| x == "factory-secrets") { return; }
        parts::get(cx, "/api/secrets".into(), move |j, _| {
            if with(|e| e.gn != gn) || !b(&j, "ok") { return; }
            with(|e| e.secrets = Some(arr(&j, "secrets").to_vec()));
            render_model();
        });
    });
}

// ─── Changes, undo, redo ───

fn mutate(key: &str, cx: &mut App, f: impl FnOnce(&mut Value)) {
    let ok = with(|e| {
        if !e.open || e.state != "ok" { return false; }
        let now = model::now_ms();
        let coalesce = !key.is_empty() && e.last_key == key && now - e.last_at < 1500.0;
        e.last_key = key.to_string(); e.last_at = now;
        if !coalesce { e.past.push(e.f.clone()); if e.past.len() > 100 { e.past.remove(0); } e.future.clear(); }
        f(&mut e.f);
        e.dirty = true; e.save = "dirty".into();
        true
    });
    if ok { schedule_save(cx); render_model(); }
}

fn undo(cx: &mut App) {
    let ok = with(|e| { let Some(p) = e.past.pop() else { return false }; let cur = std::mem::replace(&mut e.f, p); e.future.push(cur); e.last_key.clear(); e.dirty = true; e.save = "dirty".into(); true });
    if ok { schedule_save(cx); render_model(); }
}
fn redo(cx: &mut App) {
    let ok = with(|e| { let Some(p) = e.future.pop() else { return false }; let cur = std::mem::replace(&mut e.f, p); e.past.push(cur); e.last_key.clear(); e.dirty = true; e.save = "dirty".into(); true });
    if ok { schedule_save(cx); render_model(); }
}

// ─── Saving ───

fn schedule_save(cx: &mut App) {
    let gn = with(|e| { e.save_gen += 1; e.save_gen });
    parts::after(cx, 1200, move |cx| { if with(|e| e.save_gen == gn) { save_now(cx, |_, _| {}); } });
}

/// doSave(): true when saved (or nothing to save).
fn save_now(cx: &mut App, then: impl FnOnce(bool, &mut App) + 'static) {
    let job = with(|e| {
        e.save_gen += 1;
        if !e.dirty || !e.open { return None; }
        e.dirty = false; e.save = "saving".into();
        Some((e.id.clone(), e.f.clone(), e.version, e.gn))
    });
    let Some((id, f, version, gn)) = job else { then(true, cx); return };
    push_ed();
    parts::post(cx, "/api/factories", json!({ "action": "save", "id": id, "definition": f, "baseVersion": version }), move |r, cx| {
        let saved = if with(|e| e.gn != gn) { false } else if b(&r, "ok") {
            with(|e| { e.version = n(&r["factory"], "version") as i64; if r.get("validation").is_some() { e.validation = r["validation"].clone(); } e.save = if e.dirty { "dirty".into() } else { "saved".into() }; });
            true
        } else if b(&r, "conflict") { with(|e| e.save = "conflict".into()); false } else { with(|e| { e.dirty = true; e.save = "error".into(); }); false };
        render_model();
        then(saved, cx);
    });
}

fn reload(cx: &mut App) {
    let (id, gn) = with(|e| (e.id.clone(), e.gn));
    parts::get(cx, format!("/api/factories/get?id={}", parts::enc(&id)), move |j, _| {
        if with(|e| e.gn != gn) || !b(&j, "ok") { return; }
        with(|e| { e.f = j["factory"].clone(); e.version = n(&j["factory"], "version") as i64; e.validation = j["validation"].clone(); e.past.clear(); e.future.clear(); e.dirty = false; e.save = "saved".into(); });
        render_model();
    });
}

fn flash(t: &str, cx: &mut App) {
    let gn = with(|e| { e.toast = t.to_string(); e.toast_gen += 1; e.toast_gen });
    push_ed();
    parts::after(cx, 4000, move |_| { if with(|e| e.toast_gen == gn) { with(|e| e.toast.clear()); push_ed(); } });
}

// ─── Building what it shows ───

fn push_ed() {
    with(|e| {
        let ok = b(&e.validation, "ok");
        let errs = arr(&e.validation, "errors").len();
        let old = e.ed.clone();
        e.ed = Ed {
            state: e.state.clone(), error: e.error.clone(), name: s(&e.f, "name"), save: e.save.clone(),
            save_label: match e.save.as_str() { "saving" => "Saving…", "dirty" => "Unsaved", "conflict" => "Changed elsewhere", "error" => "Not saved", _ => "Saved" }.into(),
            draft: e.state == "ok" && !ok, can_undo: !e.past.is_empty(), can_redo: !e.future.is_empty(), tab: e.tab.clone(),
            issues_label: if ok { "Issues".to_string() } else { format!("Fix {errs}") }, issues_bad: !ok && e.state == "ok", toast: e.toast.clone(),
            zoom: e.z, pan_x: e.px, pan_y: e.py, bx: old.bx, by: old.by, bw: old.bw, bh: old.bh, conn: e.conn.is_some(), conn_path: e.conn.clone().unwrap_or_default(),
        };
    });
}

fn known_skills() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    for (nm, d, _) in with(|e| e.skills.clone()) { if !out.iter().any(|x| x.0 == nm) { out.push((nm, d)); } }
    out
}
fn mentions(text: &str) -> Vec<Check> {
    let known = known_skills();
    model::mentions(text).into_iter().map(|m| { let k = known.iter().find(|x| x.0 == m); Check { id: m.clone(), label: m, sub: k.map(|x| x.1.clone()).unwrap_or_default(), on: k.is_some(), enabled: true } }).collect()
}

fn render_model() {
    let st = with(|e| e.state.clone());
    if st != "ok" { push_ed(); return; }
    let (f, validation, sel) = with(|e| (e.f.clone(), e.validation.clone(), e.sel.clone()));
    let (by_node, by_edge, _) = model::issues_by(&validation);
    let mut nodes = vec![];
    let mut rects: HashMap<String, Rect> = HashMap::new();
    let mut outs_of: HashMap<String, Vec<String>> = HashMap::new();
    for node in arr(&f, "nodes") {
        let id = s(node, "id");
        let ty = { let t = s(node, "type"); if ["start", "manager", "employee", "decision", "join", "ask", "wait", "end"].contains(&t.as_str()) { t } else { "employee".into() } };
        let issues = by_node.get(&id).cloned().unwrap_or_default();
        let (badge, level) = model::badge_of(&issues);
        let body = model::node_body(node, &f);
        let outs = model::outputs_of(node);
        let w = model::node_w(&ty);
        let lines = model::body_lines(&body, w);
        let h = model::node_h(lines, false, outs.len());
        let r = Rect { x: n(&node["position"], "x") as f32, y: n(&node["position"], "y") as f32, w, h };
        rects.insert(id.clone(), r);
        nodes.push(Node {
            id: id.clone(), kind: ty.clone(), kind_label: model::kind_label(&ty).to_uppercase(), title: model::node_title(node, &f), body, lines,
            x: r.x, y: r.y, w, h, sel: sel.as_ref().map(|x| x.0 == "node" && x.1 == id).unwrap_or(false), bad: issues.iter().any(|i| i.0 == "error"),
            badge, badge_level: level,
            outs: outs.iter().map(|h2| Out { id: h2.clone(), label: model::handle_label(node, h2), finish: h2 == "finish" }).collect(),
            agent: model::is_agent(&ty), has_in: ty != "start", ..Default::default()
        });
        outs_of.insert(id, outs);
    }
    let lanes = model::lanes(&f);
    let mut edges = vec![];
    for e in arr(&f, "edges") {
        let (src, tgt, id) = (s(e, "source"), s(e, "target"), s(e, "id"));
        let (Some(sr), Some(tr)) = (rects.get(&src), rects.get(&tgt)) else { continue };
        let note = s(e, "kind") == "note";
        let sh_name = s(e, "sourceHandle");
        let (sx, sy) = model::out_point(sr, outs_of.get(&src).map(|v| v.as_slice()).unwrap_or(&[]), if note { "note" } else { sh_name.as_str() });
        let (tx, ty) = model::in_point(tr, note);
        let (path, lx, ly) = match lanes.get(&id) { Some(l) if !note => model::loop_path(sx, sy, tx, ty, *l), _ => model::bezier(sx, sy, tx, ty, note) };
        let bad = by_edge.get(&id).map(|l| l.iter().any(|i| i.0 == "error")).unwrap_or(false);
        let tone = if bad { "bad" } else if n(e, "maxRounds") > 0.0 && !note { "loop" } else if note { "note" } else { "flow" };
        edges.push(Edge {
            id: id.clone(), path, arrow: model::arrow(tx, ty, note), tone: tone.into(), sel: sel.as_ref().map(|x| x.0 == "edge" && x.1 == id).unwrap_or(false), pulse: false,
            label: model::edge_label(e, None), icon: if n(e, "maxRounds") > 0.0 && !note { "loop".into() } else if note { "note".into() } else { String::new() }, lx, ly,
        });
    }
    let bounds = model::union(&rects.values().copied().collect::<Vec<_>>()).unwrap_or(Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 });
    let people: Vec<Opt> = arr(&f, "roster").iter().filter(|r| s(r, "id") != "manager").map(|r| Opt { id: s(r, "id"), label: { let x = s(r, "name"); if x.is_empty() { "Unnamed".into() } else { x } } }).collect();
    let fit_now = with(|e| { e.rects = rects; e.outs = outs_of; !e.fitted && e.cw > 0.0 });
    if fit_now { let (cw, ch) = with(|e| (e.cw, e.ch)); let (z, px, py) = model::fit(bounds, cw, ch, 0.15, 0.15, 1.0); with(|e| { e.z = z; e.px = px; e.py = py; e.fitted = true; }); }
    let (key, insp) = inspector(&f, &validation);
    with(|e| {
        if e.insp_key != key { e.insp_key = key.clone(); }
        e.nodes = nodes; e.edges = edges; e.people = people; e.insp = Some(insp);
    });
    push_ed();
    with(|e| { e.ed.bx = bounds.x; e.ed.by = bounds.y; e.ed.bw = bounds.w; e.ed.bh = bounds.h; });
}

fn issue_rows(list: &[model::Issue]) -> Vec<Issue> {
    list.iter().map(|(lv, m, fix, raw)| {
        let key = { let e = s(raw, "employeeId"); if !e.is_empty() { format!("emp:{e}") } else { let nid = { let x = s(raw, "nodeId"); if !x.is_empty() { x } else { arr(raw, "nodeIds").first().and_then(|v| v.as_str()).unwrap_or("").to_string() } }; if !nid.is_empty() { format!("node:{nid}") } else { let ed = s(raw, "edgeId"); if ed.is_empty() { String::new() } else { format!("edge:{ed}") } } } };
        Issue { level: lv.clone(), message: m.clone(), fix: fix.clone(), key }
    }).collect()
}

fn presets_list() -> Vec<(String, String)> { arr(&data(), "presets").iter().map(|p| (s(p, "key"), s(p, "label"))).collect() }
fn str_list(v: &Value) -> Vec<String> { v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default() }

/// The side's pane: (its key, its row).
fn inspector(f: &Value, validation: &Value) -> (String, Insp) {
    let (tab, sel, roster_sel, secrets) = with(|e| (e.tab.clone(), e.sel.clone(), e.roster_sel.clone(), e.secrets.clone()));
    let (by_node, by_edge, by_emp) = model::issues_by(validation);
    let mut d = Insp::default();
    let roster = arr(f, "roster");
    let settings = &f["settings"];
    if tab == "team" {
        if let Some(p) = roster_sel.as_ref().and_then(|id| roster.iter().find(|r| s(r, "id") == *id)) {
            let id = s(p, "id");
            let mgr = id == "manager";
            d.mode = "person".into();
            d.manager = mgr;
            d.issues = issue_rows(by_emp.get(&id).map(|v| v.as_slice()).unwrap_or(&[]));
            d.name = s(p, "name"); d.persona = s(p, "persona"); d.mentions_p = mentions(&s(p, "persona"));
            d.model_i = ["", "opus", "sonnet", "haiku"].iter().position(|x| *x == s(p, "model")).unwrap_or(0);
            d.effort_i = ["", "low", "medium", "high", "xhigh", "max"].iter().position(|x| *x == s(p, "effort")).unwrap_or(0);
            let fresh = s(p, "memory") == "fresh";
            d.memory_i = fresh as usize;
            d.memory_hint = if fresh { "A new session each time: useful for reviewers, who should look with fresh eyes." } else { "One session for the whole run: they remember earlier steps." }.into();
            let deny = str_list(&p["tools"]["deny"]);
            d.can_edit = !EDIT_TOOLS.iter().any(|t| deny.iter().any(|x| x == t));
            d.pinned = str_list(&p["skills"]).join(", ");
            let everyone = str_list(&settings["presets"]);
            let mine = str_list(&p["presets"]);
            d.show_presets = !mgr && s(settings, "teamMode") != "bypass";
            d.p_presets = presets_list().into_iter().map(|(k, l)| { let all = everyone.contains(&k); Check { id: k.clone(), label: format!("{l}{}", if all { " (everyone has this)" } else { "" }), sub: String::new(), on: all || mine.contains(&k), enabled: !all } }).collect();
            d.allow = str_list(&p["tools"]["allow"]).join(", "); d.deny = deny.join(", ");
            let used = model::steps_of(f, &id);
            d.steps_hint = format!("In {used} step{} of the flow.", if used == 1 { "" } else { "s" });
            return (format!("person:{id}"), d);
        }
        d.mode = "team".into();
        d.people = roster.iter().map(|r| { let id = s(r, "id"); Person { id: id.clone(), name: { let x = s(r, "name"); if x.is_empty() { "Unnamed".into() } else { x } }, meta: model::person_meta(f, &id, by_emp.get(&id).map(|l| l.len()).unwrap_or(0)), manager: id == "manager" } }).collect();
        let mut opts: Vec<Opt> = arr(&data(), "personas").iter().map(|p| Opt { id: s(p, "key"), label: s(p, "name") }).collect();
        opts.push(Opt { id: String::new(), label: "Blank".into() });
        d.personas = opts;
        return ("team".into(), d);
    }
    let ok = b(validation, "ok");
    if tab == "issues" {
        let mut all = vec![];
        for (list, lv) in [("errors", "error"), ("warnings", "warning")] { for it in arr(validation, list) { all.push((lv.to_string(), s(it, "message"), s(it, "fix"), it.clone())); } }
        d.mode = "issues".into(); d.ok = ok;
        d.issues_title = if ok { "Ready to run".to_string() } else { format!("{} to fix", arr(validation, "errors").len()) };
        d.issues_hint = if ok && !all.is_empty() { "It can run. The warnings below are worth a look." } else if !ok { "It is saved as a draft and cannot run until these are fixed. Tap one to go to it." } else { "" }.into();
        d.issues = issue_rows(&all);
        return ("issues".into(), d);
    }
    if tab == "dry" { d.mode = "dry".into(); d.ok = ok; return ("dry".into(), d); }
    let names = |n2: &Value| -> String { let l = s(n2, "label"); if !l.is_empty() { return l; } if s(n2, "type") == "employee" { if let Some(r) = roster.iter().find(|r| s(r, "id") == s(n2, "employeeId")) { let x = s(r, "name"); if !x.is_empty() { return x; } } } model::kind_label(&s(n2, "type")).to_string() };
    if let Some((kind, id)) = sel.clone() {
        if kind == "node" {
            if let Some(node) = arr(f, "nodes").iter().find(|x| s(x, "id") == id) {
                let ty = s(node, "type");
                d.mode = "node".into(); d.kind = ty.clone(); d.kind_label = model::kind_label(&ty).into(); d.can_delete = !["start", "manager", "end"].contains(&ty.as_str());
                d.issues = issue_rows(by_node.get(&id).map(|v| v.as_slice()).unwrap_or(&[]));
                d.label = s(node, "label"); d.label_ph = model::kind_label(&ty).into();
                let emps: Vec<&Value> = roster.iter().filter(|r| s(r, "id") != "manager").collect();
                let mut who = vec!["Choose someone…".to_string()];
                who.extend(emps.iter().map(|r| { let x = s(r, "name"); if x.is_empty() { "Unnamed".into() } else { x } }));
                who.push("+ Add someone to the team…".into());
                d.who = who;
                let eid = s(node, "employeeId");
                d.who_i = emps.iter().position(|r| s(r, "id") == eid).map(|i| i + 1).unwrap_or(0);
                d.has_person = !eid.is_empty();
                let dc = &node["decision"];
                d.question = s(dc, "question"); d.mentions_q = mentions(&s(if ty == "ask" { &node["ask"] } else { dc }, if ty == "ask" { "prompt" } else { "question" }));
                d.dkind_i = match s(dc, "kind").as_str() { "one" => 1, "many" => 2, _ => 0 };
                d.options = arr(dc, "options").iter().map(|o| Opt { id: s(o, "id"), label: s(o, "label") }).collect();
                d.review = b(dc, "review");
                let mut ans = vec!["Whoever arrives".to_string()];
                ans.extend(roster.iter().map(|r| { let x = s(r, "name"); if x.is_empty() { s(r, "id") } else { x } }));
                d.answered = ans;
                let ab = s(dc, "answeredBy");
                d.answered_i = roster.iter().position(|r| s(r, "id") == ab && !ab.is_empty()).map(|i| i + 1).unwrap_or(0);
                d.prompt = s(&node["ask"], "prompt");
                d.replies = arr(&node["ask"], "replies").iter().map(|o| Opt { id: s(o, "id"), label: s(o, "label") }).collect();
                let w = &node["wait"];
                d.wkind_i = match s(w, "kind").as_str() { "signal" => 1, "time" => 2, "until" => 3, _ => 0 };
                let num = |k: &str| { let v = n(w, k) as i64; if w.get(k).map(|x| x.is_null()).unwrap_or(true) { String::new() } else { v.to_string() } };
                d.minutes = num("minutes"); d.at = s(w, "at"); d.command = s(w, "command"); d.every = num("every"); d.what = s(w, "what"); d.timeout = num("timeout");
                let forks: Vec<&Value> = arr(f, "nodes").iter().filter(|x| s(x, "type") == "decision" && s(&x["decision"], "kind") == "many").collect();
                let mut fl = vec!["Choose a decision…".to_string()];
                fl.extend(forks.iter().map(|x| { let l = s(x, "label"); if !l.is_empty() { l } else { let q = s(&x["decision"], "question"); if q.is_empty() { "Decision".into() } else { q } } }));
                d.forks = fl;
                let fid = s(&node["join"], "forkId");
                d.fork_i = forks.iter().position(|x| s(x, "id") == fid && !fid.is_empty()).map(|i| i + 1).unwrap_or(0);
                return (format!("node:{id}"), d);
            }
        } else if let Some(e) = arr(f, "edges").iter().find(|x| s(x, "id") == id) {
            let nodes = arr(f, "nodes");
            let src = nodes.iter().find(|x| s(x, "id") == s(e, "source"));
            let dst = nodes.iter().find(|x| s(x, "id") == s(e, "target"));
            let note = s(e, "kind") == "note";
            let nm = |x: Option<&Value>| x.map(names).unwrap_or_else(|| "?".into());
            let sh = s(e, "sourceHandle");
            d.mode = "edge".into(); d.note = note; d.can_delete = sh != "finish";
            d.edge_desc = format!("{}{} → {}", nm(src), if !note && src.is_some() && sh != "out" { format!(" ({})", model::handle_label(src.unwrap(), &sh)) } else { String::new() }, nm(dst));
            d.label = format!("A note carries a message, not control: it waits in {}'s inbox until the flow reaches them.", nm(dst));
            d.issues = issue_rows(by_edge.get(&id).map(|v| v.as_slice()).unwrap_or(&[]));
            d.note_label = s(e, "noteLabel");
            let mr = n(e, "maxRounds") as i64;
            d.rounds = if mr > 0 { mr.to_string() } else { String::new() };
            d.require_note = b(e, "requireNote");
            return (format!("edge:{id}"), d);
        }
    }
    d.mode = "settings".into();
    d.name = s(f, "name"); d.description = s(f, "description"); d.mentions_d = mentions(&s(f, "description"));
    let crt = arr(settings, "crTemplate");
    d.cr = crt.iter().map(|x| CrSec { id: s(x, "id"), label: s(x, "label"), hint: s(x, "hint"), required: b(x, "required"), list: b(x, "list") }).collect();
    let pick = |v: &[Value]| v.iter().map(|x| json!({ "id": s(x, "id"), "label": s(x, "label"), "hint": s(x, "hint"), "required": b(x, "required"), "list": b(x, "list") })).collect::<Vec<_>>();
    let dd = data();
    d.cr_default = pick(crt) == pick(arr(&dd, "crTemplate"));
    d.cr_full = crt.len() as f64 >= n(&dd["crLimits"], "sections").max(1.0);
    let bypass = s(settings, "managerMode") == "bypass";
    let tbypass = s(settings, "teamMode") == "bypass";
    let gk = b(&settings["gatekeeper"], "enabled");
    d.mmode_i = bypass as usize;
    d.mmode_hint = if bypass { "The Manager runs unchecked and approves its team's requests itself. Choose this only if you trust it. Some things stay refused: sudo, restarting Crundi, pushing, the server's ports." } else { "The Manager's own actions ask you. It still decides its team's permission requests." }.into();
    d.tmode_i = tbypass as usize;
    d.gatekeeper = gk;
    d.tmode_hint = if tbypass { "The team runs unchecked too: nobody asks for anything, which is fastest. The fixed refusals still apply, but nothing stops them writing outside their worktree, so only for projects you trust them with." } else if gk { "Requests go to the Gatekeeper, which passes the unclear ones to the Manager." } else { "Requests go to the Manager, whose decision is final." }.into();
    d.bypass_warn = bypass && !tbypass;
    let everyone = str_list(&settings["presets"]);
    d.presets = presets_list().into_iter().map(|(k, l)| Check { on: everyone.contains(&k), id: k, label: l, sub: String::new(), enabled: true }).collect();
    d.rules = str_list(&settings["rules"]).join("\n");
    d.manager_rules = b(settings, "managerRules");
    d.mrules_hint = if d.manager_rules { "So it is not asked the same thing again and again: a rule it grants applies to that person (or everyone, if it says so) for the rest of the run." } else { "It answers every request on its own." }.into();
    d.gk_i = ["haiku", "sonnet", "opus"].iter().position(|x| *x == s(&settings["gatekeeper"], "model")).unwrap_or(0);
    if let Some(list) = secrets {
        let chosen = str_list(&settings["secrets"]);
        d.secrets_on = true;
        d.secrets = list.iter().map(|x| Check { id: s(x, "id"), label: s(x, "name"), sub: s(x, "description"), on: chosen.contains(&s(x, "id")), enabled: true }).collect();
        d.secrets_gone = chosen.iter().filter(|id| !list.iter().any(|x| s(x, "id") == **id)).cloned().collect();
    }
    d.merge_i = (s(settings, "mergeMode") == "auto") as usize;
    d.auto_recover = b(settings, "autoRecover");
    d.setup = s(settings, "setupCommand");
    d.max_handoffs = (n(&settings["caps"], "maxHandoffs") as i64).to_string();
    d.max_minutes = (n(&settings["caps"], "maxMinutes") as i64).to_string();
    let tick = n(settings, "tickMinutes") as i64;
    d.tick = if tick > 0 { tick.to_string() } else { String::new() };
    d.tick_hint = if tick > 0 { format!("A wake-up every {tick} minutes, only while the Manager is idle; skipped otherwise. Each one is a short Manager turn.") } else { "Blank: the Manager chooses at the start of each run (5 minutes to 24 hours). A wake-up only while it is idle, so it can check on things.".into() };
    let lim = &settings["limits"];
    d.l5h = (n(lim, "pauseAt5h") as i64).to_string(); d.lweek = (n(lim, "pauseAtWeek") as i64).to_string();
    d.lpar = (n(lim, "maxParallel") as i64).to_string(); d.lone = (n(lim, "oneAtATimeAbove") as i64).to_string();
    d.lim_hint = if n(lim, "pauseAt5h") < 100.0 || n(lim, "pauseAtWeek") < 100.0 { "It pauses there and carries on by itself after the window resets, leaving the rest for you." } else { "At 100% it pauses only when a window is full, and carries on by itself after the reset." }.into();
    ("settings".into(), d)
}

// ─── The canvas ───

pub fn size(w: f32, h: f32) {
    let first = with(|e| { let f = !e.fitted; e.cw = w; e.ch = h; f && e.state == "ok" });
    if first { render_model(); }
}
pub fn pan(dx: f32, dy: f32) { with(|e| { e.px += dx; e.py += dy; }); push_ed(); }
pub fn zoom_at(d: f32, mx: f32, my: f32) {
    with(|e| { let z2 = (e.z * (d * 0.0025).exp()).clamp(0.15, 2.0); e.px = mx - (mx - e.px) * z2 / e.z; e.py = my - (my - e.py) * z2 / e.z; e.z = z2; });
    push_ed();
}
pub fn control(k: &str) {
    with(|e| {
        let (cx, cy) = (e.cw / 2.0, e.ch / 2.0);
        if k == "fit" {
            if let Some(bx) = model::union(&e.rects.values().copied().collect::<Vec<_>>()) { let (z, px, py) = model::fit(bx, e.cw, e.ch, 0.15, 0.15, 1.0); e.z = z; e.px = px; e.py = py; }
        } else {
            let z2 = if k == "zoom-in" { (e.z * 1.2).min(2.0) } else { (e.z / 1.2).max(0.15) };
            e.px = cx - (cx - e.px) * z2 / e.z; e.py = cy - (cy - e.py) * z2 / e.z; e.z = z2;
        }
    });
    push_ed();
}
pub fn mini(fx: f32, fy: f32) { with(|e| { e.px = e.cw / 2.0 - fx * e.z; e.py = e.ch / 2.0 - fy * e.z; }); push_ed(); }

fn select(kind: &str, id: &str) {
    with(|e| { e.sel = Some((kind.to_string(), id.to_string())); e.tab = "edit".into(); });
    render_model();
}

pub fn node_down(id: &str) {
    let pos = with(|e| arr(&e.f, "nodes").iter().find(|x| s(x, "id") == id).map(|x| (n(&x["position"], "x"), n(&x["position"], "y"))));
    let Some((x, y)) = pos else { return };
    with(|e| { e.drag = Some((id.to_string(), x, y, e.f.clone(), false)); });
    select("node", id);
}
pub fn node_drag(id: &str, dx: f32, dy: f32) {
    let ok = with(|e| {
        let z = e.z as f64;
        let Some((did, x0, y0, _, _)) = e.drag.clone() else { return false };
        if did != id { return false; }
        let (nx, ny) = (((x0 + dx as f64 / z) / 16.0).round() * 16.0, ((y0 + dy as f64 / z) / 16.0).round() * 16.0);
        if let Some(nodes) = e.f["nodes"].as_array_mut() { if let Some(nd) = nodes.iter_mut().find(|x| s(x, "id") == id) { nd["position"] = json!({ "x": nx, "y": ny }); } }
        if let Some(d) = e.drag.as_mut() { d.4 = true; }
        true
    });
    if ok { render_model(); }
}
pub fn node_up(_id: &str, moved: bool, cx: &mut App) {
    let changed = with(|e| {
        let Some((_, _, _, snap, did)) = e.drag.take() else { return false };
        if !(moved && did) || snap == e.f { return false; }
        e.past.push(snap); if e.past.len() > 100 { e.past.remove(0); } e.future.clear(); e.dirty = true; e.save = "dirty".into(); e.last_key.clear();
        true
    });
    if changed { schedule_save(cx); render_model(); }
}

fn flow_of(x: f32, y: f32) -> (f32, f32) { with(|e| ((x - e.px) / e.z, (y - e.py) / e.z)) }

pub fn handle_drag(node: &str, h: &str, x: f32, y: f32) {
    let (tx, ty) = flow_of(x, y);
    let path = with(|e| {
        let r = e.rects.get(node)?;
        let (sx, sy) = model::out_point(r, e.outs.get(node).map(|v| v.as_slice()).unwrap_or(&[]), h);
        Some(model::bezier(sx, sy, tx, ty, h == "note").0)
    });
    if let Some(p) = path { with(|e| e.conn = Some(p)); push_ed(); }
}

pub fn handle_up(node: &str, h: &str, x: f32, y: f32, cx: &mut App) {
    let (fx, fy) = flow_of(x, y);
    let target = with(|e| {
        e.conn = None;
        let pad = 30.0 / e.z;
        e.rects.iter().filter(|(id, _)| id.as_str() != node).find(|(_, r)| fx >= r.x - pad && fx <= r.x + r.w + pad && fy >= r.y - pad && fy <= r.y + r.h + pad).map(|(id, _)| id.clone())
    });
    push_ed();
    if let Some(t) = target { connect_ids(node, h, &t, cx); }
}

/// onConnect(): a valid connection is added; one that closes a loop gets a limit.
pub fn connect_ids(src: &str, h: &str, tgt: &str, cx: &mut App) -> bool {
    let th = if h == "note" { "notein" } else { "in" };
    let ok = with(|e| model::can_connect(&e.f, src, h, tgt, th));
    if !ok { return false; }
    let looped = Rc::new(RefCell::new(false));
    let l2 = looped.clone();
    let (src, h, tgt) = (src.to_string(), h.to_string(), tgt.to_string());
    mutate("", cx, move |f| { *l2.borrow_mut() = model::connect(f, &src, &h, &tgt); });
    if *looped.borrow() { flash("That closes a loop, so it got a round limit of 3. Change it on the connection.", cx); }
    true
}

pub fn edge_click(id: &str) { select("edge", id); }
pub fn pane_click() { if with(|e| e.sel.is_some()) { with(|e| e.sel = None); render_model(); } }

pub fn key(k: &str, cx: &mut App) {
    if k != "delete" { return; }
    let sel = with(|e| e.sel.clone());
    match sel {
        Some((kind, id)) if kind == "node" => {
            let deletable = with(|e| arr(&e.f, "nodes").iter().any(|x| s(x, "id") == id && !["start", "manager", "end"].contains(&s(x, "type").as_str())));
            if deletable { mutate("", cx, |f| model::remove_nodes(f, &[id])); with(|e| e.sel = None); render_model(); }
        }
        Some((_, id)) => { mutate("", cx, |f| model::remove_edge(f, &id)); with(|e| e.sel = None); render_model(); }
        None => {}
    }
}

/// addAt(): a new step at a point of the canvas (the middle of the view when none).
fn add_at(kind: &str, at: Option<(f32, f32)>, cx: &mut App) {
    let (fx, fy) = match at { Some((x, y)) => flow_of(x, y), None => { let (cw, ch) = with(|e| (e.cw, e.ch)); flow_of(cw / 2.0, ch / 2.0) } };
    let (sx, sy) = (((fx / 16.0).round() * 16.0) as f64, ((fy / 16.0).round() * 16.0) as f64);
    let node = model::new_node(kind, sx, sy);
    let id = s(&node, "id");
    let k2 = kind.to_string();
    mutate("", cx, move |f| {
        let mut nodes = arr(f, "nodes").to_vec();
        nodes.push(node.clone());
        if k2 == "decision:many" { let mut j = model::new_node("join", sx, sy + 240.0); j["label"] = json!("Join"); j["join"] = json!({ "forkId": s(&node, "id") }); nodes.push(j); }
        f["nodes"] = Value::Array(nodes);
    });
    with(|e| {
        e.sel = Some(("node".into(), id.clone()));
        if kind == "employee:" { e.roster_add_to = Some(id.clone()); e.roster_sel = None; e.tab = "team".into(); } else { e.tab = "edit".into(); }
    });
    render_model();
}

fn tidy(cx: &mut App) {
    let heights: HashMap<String, f32> = with(|e| e.rects.iter().map(|(k, r)| (k.clone(), r.h)).collect());
    let pos = with(|e| model::tidy(&e.f, &heights));
    mutate("", cx, move |f| {
        if let Some(nodes) = f["nodes"].as_array_mut() { for nd in nodes.iter_mut() { if let Some((x, y)) = pos.get(&s(nd, "id")) { nd["position"] = json!({ "x": x, "y": y }); } } }
    });
    control("fit");
}

fn pick_issue(key: &str) {
    if let Some(id) = key.strip_prefix("emp:") { with(|e| { e.roster_sel = Some(id.to_string()); e.tab = "team".into(); }); render_model(); return; }
    if let Some(id) = key.strip_prefix("node:") {
        select("node", id);
        with(|e| if let Some(r) = e.rects.get(id).copied() { let z = e.z.max(0.9); e.z = z; e.px = e.cw / 2.0 - (r.x + r.w / 2.0) * z; e.py = e.ch / 2.0 - (r.y + 40.0) * z; });
        push_ed();
        return;
    }
    if let Some(id) = key.strip_prefix("edge:") { select("edge", id); }
}

fn remove_person(cx: &mut App) {
    let Some(id) = with(|e| e.roster_sel.clone()) else { return };
    let (name, steps) = with(|e| (arr(&e.f, "roster").iter().find(|r| s(r, "id") == id).map(|r| s(r, "name")).filter(|x| !x.is_empty()).unwrap_or_else(|| "this person".into()), model::steps_of(&e.f, &id)));
    let msg = format!("Remove {name} from the team{}?", if steps > 0 { format!(" and their {steps} step{} from the flow", if steps == 1 { "" } else { "s" }) } else { String::new() });
    parts::confirm(cx, "Remove", &msg, "Remove", true, move |ok, cx| {
        if !ok { return; }
        let id2 = id.clone();
        mutate("", cx, move |f| {
            let gone: Vec<String> = arr(f, "nodes").iter().filter(|x| s(x, "type") == "employee" && s(x, "employeeId") == id2).map(|x| s(x, "id")).collect();
            let set: HashSet<String> = gone.iter().cloned().collect();
            let nodes: Vec<Value> = arr(f, "nodes").iter().filter(|x| !set.contains(&s(x, "id"))).cloned().map(|mut x| { if s(&x, "type") == "decision" && s(&x["decision"], "answeredBy") == id2 { x["decision"]["answeredBy"] = Value::Null; } x }).collect();
            let edges: Vec<Value> = arr(f, "edges").iter().filter(|e| !set.contains(&s(e, "source")) && !set.contains(&s(e, "target"))).cloned().collect();
            let roster: Vec<Value> = arr(f, "roster").iter().filter(|r| s(r, "id") != id2).cloned().collect();
            f["nodes"] = Value::Array(nodes); f["edges"] = Value::Array(edges); f["roster"] = Value::Array(roster);
        });
        with(|e| e.roster_sel = None);
        render_model();
    });
}

fn add_person(key: &str, cx: &mut App) {
    let dd = data();
    let tpl = arr(&dd, "personas").iter().find(|p| s(p, "key") == key).cloned();
    let skills: Vec<String> = known_skills().into_iter().map(|x| x.0).collect();
    let emp = with(|e| model::new_employee(tpl.as_ref(), &e.f, if skills.is_empty() { None } else { Some(skills.as_slice()) }));
    let id = s(&emp, "id");
    let add_to = with(|e| e.roster_add_to.clone());
    let id2 = id.clone();
    mutate("", cx, move |f| {
        let mut r = arr(f, "roster").to_vec(); r.push(emp); f["roster"] = Value::Array(r);
        // The step it was added for now names them.
        if let Some(nid) = add_to { if let Some(nd) = node_mut(f, &nid) { nd["employeeId"] = json!(id2); } }
    });
    with(|e| e.roster_sel = Some(id));
    render_model();
}

pub fn act(a: &str, x: &str, y: &str, cx: &mut App) {
    match a {
        "e-back" => save_now(cx, |_, cx| super::close_sub(cx)),
        "e-save" => { with(|e| e.dirty = true); save_now(cx, |_, _| {}); }
        "e-reload" => reload(cx),
        "e-undo" => undo(cx),
        "e-redo" => redo(cx),
        "e-tidy" => tidy(cx),
        "e-settings" => { with(|e| { e.sel = None; e.tab = "edit".into(); }); render_model(); }
        "e-tab" => { with(|e| e.tab = x.to_string()); render_model(); }
        "e-add" => add_at(x, None, cx),
        "e-add-at" => {
            let p: Vec<f32> = y.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(-1.0)).collect();
            let (px, py) = (p.first().copied().unwrap_or(-1.0), p.get(1).copied().unwrap_or(-1.0));
            let (cw, ch) = with(|e| (e.cw, e.ch));
            if px >= 0.0 && py >= 0.0 && px <= cw && py <= ch { add_at(x, Some((px, py)), cx); }
        }
        "e-delete" => key("delete", cx),
        "e-person" => {
            let id = if x.is_empty() { with(|e| e.sel.clone().and_then(|(k, id)| if k == "node" { arr(&e.f, "nodes").iter().find(|n2| s(n2, "id") == id).map(|n2| s(n2, "employeeId")) } else { None })).unwrap_or_default() } else { x.to_string() };
            with(|e| { e.roster_sel = Some(id); e.tab = "team".into(); });
            render_model();
        }
        "e-team" => { with(|e| { e.roster_sel = None; e.roster_add_to = None; }); render_model(); }
        "e-add-person" => add_person(x, cx),
        "e-remove-person" => remove_person(cx),
        "e-issue" => pick_issue(x),
        "e-cr-open" => with(|e| e.cr_open = !e.cr_open),
        "e-more" => with(|e| e.more_open = !e.more_open),
        _ => {}
    }
}

// ─── The fields ───

fn clamp_num(v: &str, lo: i64, hi: i64) -> i64 { let t = v.trim(); if t.is_empty() { 0 } else { t.parse::<f64>().map(|x| (x.round() as i64).clamp(lo, hi)).unwrap_or(lo) } }
fn list_of(v: &str, sep: char) -> Value { json!(v.split(sep).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect::<Vec<_>>()) }
fn set_in(f: &mut Value, path: &[&str], v: Value) { let mut cur = f; for p in &path[..path.len() - 1] { if !cur[*p].is_object() { cur[*p] = json!({}); } cur = &mut cur[*p]; } cur[path[path.len() - 1]] = v; }

/// A field of the pane changed (Inspector.jsx's set()).
pub fn edit(k: &str, v: &str, cx: &mut App) {
    let (sel, roster_sel, tab) = with(|e| (e.sel.clone(), e.roster_sel.clone(), e.tab.clone()));
    let v2 = v.to_string();
    if tab == "team" {
        let Some(pid) = roster_sel else { return };
        let key = format!("{k}:{pid}");
        let pid2 = pid.clone();
        let k2 = k.to_string();
        mutate(if ["p-name", "persona", "pinned", "allow", "deny"].contains(&k) { key.as_str() } else { "" }, cx, move |f| {
            let everyone: Vec<String> = str_list(&f["settings"]["presets"]);
            let Some(r) = f["roster"].as_array_mut().and_then(|a| a.iter_mut().find(|r| s(r, "id") == pid2)) else { return };
            match k2.as_str() {
                "p-name" => r["name"] = json!(v2),
                "persona" => r["persona"] = json!(v2),
                "model" => r["model"] = json!(["", "opus", "sonnet", "haiku"].get(v2.parse::<usize>().unwrap_or(0)).copied().unwrap_or("")),
                "effort" => r["effort"] = json!(["", "low", "medium", "high", "xhigh", "max"].get(v2.parse::<usize>().unwrap_or(0)).copied().unwrap_or("")),
                "memory" => r["memory"] = json!(if v2 == "1" { "fresh" } else { "keep" }),
                "can-edit" => {
                    let deny = str_list(&r["tools"]["deny"]);
                    let next: Vec<String> = if v2 == "1" { deny.into_iter().filter(|t| !EDIT_TOOLS.contains(&t.as_str())).collect() } else { let mut d = deny; for t in EDIT_TOOLS { if !d.iter().any(|x| x == t) { d.push(t.into()); } } d };
                    set_in(r, &["tools", "deny"], json!(next));
                }
                "pinned" => r["skills"] = json!(v2.split(',').map(|x| x.trim().trim_start_matches('/').to_string()).filter(|x| !x.is_empty()).collect::<Vec<_>>()),
                "allow" => set_in(r, &["tools", "allow"], list_of(&v2, ',')),
                "deny" => set_in(r, &["tools", "deny"], list_of(&v2, ',')),
                x if x.starts_with("p-preset:") => {
                    let key = &x["p-preset:".len()..];
                    if everyone.iter().any(|e| e == key) { return; }
                    let mut p = str_list(&r["presets"]);
                    if v2 == "1" { if !p.iter().any(|x| x == key) { p.push(key.into()); } } else { p.retain(|x| x != key); }
                    r["presets"] = json!(p);
                }
                _ => {}
            }
        });
        return;
    }
    match sel {
        Some((kind, id)) if kind == "node" => {
            let id2 = id.clone();
            if k == "who" {
                let i: usize = v.parse().unwrap_or(0);
                let emps: Vec<String> = with(|e| arr(&e.f, "roster").iter().filter(|r| s(r, "id") != "manager").map(|r| s(r, "id")).collect());
                if i == emps.len() + 1 { with(|e| { e.roster_add_to = Some(id.clone()); e.roster_sel = None; e.tab = "team".into(); }); render_model(); return; }
                let eid = if i == 0 { String::new() } else { emps.get(i - 1).cloned().unwrap_or_default() };
                mutate("", cx, move |f| { if let Some(nd) = node_mut(f, &id2) { nd["employeeId"] = json!(eid); } });
                return;
            }
            let roster_ids: Vec<String> = with(|e| arr(&e.f, "roster").iter().map(|r| s(r, "id")).collect());
            let forks: Vec<String> = with(|e| arr(&e.f, "nodes").iter().filter(|x| s(x, "type") == "decision" && s(&x["decision"], "kind") == "many").map(|x| s(x, "id")).collect());
            let text = ["label", "question", "prompt", "minutes", "wat", "command", "every", "what", "timeout"].contains(&k) || k.ends_with("-label") || k.contains("-label:");
            let key = format!("{k}:{id}");
            let k2 = k.to_string();
            mutate(if text { key.as_str() } else { "" }, cx, move |f| {
                let drop_handles = |f: &mut Value, keep: &[String]| {
                    let edges: Vec<Value> = arr(f, "edges").iter().filter(|e| !(s(e, "source") == id2 && s(e, "kind") == "flow" && !keep.contains(&s(e, "sourceHandle")))).cloned().collect();
                    f["edges"] = Value::Array(edges);
                };
                match k2.as_str() {
                    "label" => { if let Some(nd) = node_mut(f, &id2) { nd["label"] = json!(v2); } }
                    "question" => { if let Some(nd) = node_mut(f, &id2) { nd["decision"]["question"] = json!(v2); } }
                    "dkind" => {
                        let kk = ["yesno", "one", "many"][v2.parse::<usize>().unwrap_or(0).min(2)];
                        let mut keep: Vec<String> = vec![];
                        if let Some(nd) = node_mut(f, &id2) {
                            nd["decision"]["kind"] = json!(kk);
                            if kk == "yesno" { nd["decision"]["options"] = json!([]); keep = vec!["yes".into(), "no".into()]; }
                            else {
                                if arr(&nd["decision"], "options").len() < 2 { nd["decision"]["options"] = json!([{ "id": model::gen_id("o_"), "label": "Option 1" }, { "id": model::gen_id("o_"), "label": "Option 2" }]); }
                                keep = arr(&nd["decision"], "options").iter().map(|o| s(o, "id")).collect();
                                if kk == "many" { keep.push("none".into()); }
                            }
                        }
                        drop_handles(f, &keep);
                    }
                    "review" => { if let Some(nd) = node_mut(f, &id2) { nd["decision"]["review"] = json!(v2 == "1"); } }
                    "answered" => { let i: usize = v2.parse().unwrap_or(0); if let Some(nd) = node_mut(f, &id2) { nd["decision"]["answeredBy"] = if i == 0 { Value::Null } else { json!(roster_ids.get(i - 1).cloned().unwrap_or_default()) }; } }
                    "prompt" => { if let Some(nd) = node_mut(f, &id2) { nd["ask"]["prompt"] = json!(v2); } }
                    "wkind" => {
                        let kk = ["command", "signal", "time", "until"][v2.parse::<usize>().unwrap_or(0).min(3)];
                        let two = |k: &str| k == "command" || k == "signal";
                        let old = node_mut(f, &id2).map(|nd| s(&nd["wait"], "kind")).unwrap_or_default();
                        if two(kk) != two(&old) { let edges: Vec<Value> = arr(f, "edges").iter().filter(|e| s(e, "source") != id2 || s(e, "kind") == "note").cloned().collect(); f["edges"] = Value::Array(edges); }
                        if let Some(nd) = node_mut(f, &id2) { nd["wait"]["kind"] = json!(kk); }
                    }
                    "minutes" => { let x = clamp_num(&v2, 1, 10080); if let Some(nd) = node_mut(f, &id2) { nd["wait"]["minutes"] = json!(if x == 0 { 1 } else { x }); } }
                    "wat" => { if let Some(nd) = node_mut(f, &id2) { nd["wait"]["at"] = json!(v2); } }
                    "command" => { if let Some(nd) = node_mut(f, &id2) { nd["wait"]["command"] = json!(v2); } }
                    "every" => { let x = clamp_num(&v2, 15, 3600); if let Some(nd) = node_mut(f, &id2) { nd["wait"]["every"] = json!(if x == 0 { 60 } else { x }); } }
                    "what" => { if let Some(nd) = node_mut(f, &id2) { nd["wait"]["what"] = json!(v2); } }
                    "timeout" => { let x = clamp_num(&v2, 1, 10080); if let Some(nd) = node_mut(f, &id2) { nd["wait"]["timeout"] = json!(if x == 0 { 60 } else { x }); } }
                    "fork" => { let i: usize = v2.parse().unwrap_or(0); if let Some(nd) = node_mut(f, &id2) { nd["join"]["forkId"] = json!(if i == 0 { String::new() } else { forks.get(i - 1).cloned().unwrap_or_default() }); } }
                    x => {
                        let (list_key, field) = if x.starts_with("options") { ("decision", "options") } else if x.starts_with("replies") { ("ask", "replies") } else { return };
                        if let Some(oid) = x.split_once("-label:").map(|p| p.1.to_string()) {
                            if let Some(nd) = node_mut(f, &id2) { if let Some(l) = nd[list_key][field].as_array_mut() { if let Some(o) = l.iter_mut().find(|o| s(o, "id") == oid) { o["label"] = json!(v2); } } }
                        } else if x.ends_with("-remove") {
                            if let Some(nd) = node_mut(f, &id2) { if let Some(l) = nd[list_key][field].as_array_mut() { l.retain(|o| s(o, "id") != v2); } }
                            let edges: Vec<Value> = arr(f, "edges").iter().filter(|e| !(s(e, "source") == id2 && s(e, "sourceHandle") == v2)).cloned().collect();
                            f["edges"] = Value::Array(edges);
                        } else if x.ends_with("-add") {
                            if let Some(nd) = node_mut(f, &id2) { if let Some(l) = nd[list_key][field].as_array_mut() { let nn = l.len() + 1; l.push(json!({ "id": model::gen_id("o_"), "label": format!("{} {nn}", if field == "options" { "Option" } else { "Reply" }) })); } }
                        }
                    }
                }
            });
        }
        Some((_, id)) => {
            let id2 = id.clone();
            let key = format!("{k}:{id}");
            let k2 = k.to_string();
            mutate(if k == "require-note" { "" } else { key.as_str() }, cx, move |f| {
                let Some(e) = f["edges"].as_array_mut().and_then(|a| a.iter_mut().find(|e| s(e, "id") == id2)) else { return };
                match k2.as_str() { "note-label" => e["noteLabel"] = json!(v2), "rounds" => e["maxRounds"] = json!(clamp_num(&v2, 0, 100)), "require-note" => e["requireNote"] = json!(v2 == "1"), _ => {} }
            });
        }
        None => edit_settings(k, v, cx),
    }
}

fn node_mut<'a>(f: &'a mut Value, id: &str) -> Option<&'a mut Value> { f["nodes"].as_array_mut().and_then(|a| a.iter_mut().find(|x| s(x, "id") == id)) }

fn edit_settings(k: &str, v: &str, cx: &mut App) {
    let v2 = v.to_string();
    let k2 = k.to_string();
    let texty = ["name", "description", "rules", "setup", "max-handoffs", "max-minutes", "tick", "l5h", "lweek", "lpar", "lone"].contains(&k) || k.starts_with("cr-label:") || k.starts_with("cr-hint:");
    mutate(if texty { k } else { "" }, cx, move |f| {
        let st = &mut f["settings"];
        let i_of = |x: &str| x.rsplit(':').next().and_then(|n2| n2.parse::<usize>().ok()).unwrap_or(0);
        match k2.as_str() {
            "name" => f["name"] = json!(v2),
            "description" => f["description"] = json!(v2),
            "mmode" => st["managerMode"] = json!(if v2 == "1" { "bypass" } else { "normal" }),
            "tmode" => st["teamMode"] = json!(if v2 == "1" { "bypass" } else { "ask" }),
            "rules" => st["rules"] = list_of(&v2, '\n'),
            "manager-rules" => st["managerRules"] = json!(v2 == "1"),
            "gatekeeper" => set_in(st, &["gatekeeper", "enabled"], json!(v2 == "1")),
            "gk" => set_in(st, &["gatekeeper", "model"], json!(["haiku", "sonnet", "opus"][v2.parse::<usize>().unwrap_or(0).min(2)])),
            "merge" => st["mergeMode"] = json!(if v2 == "1" { "auto" } else { "ask" }),
            "auto-recover" => st["autoRecover"] = json!(v2 == "1"),
            "setup" => st["setupCommand"] = json!(v2),
            "max-handoffs" => { let x = clamp_num(&v2, 1, 10000); set_in(st, &["caps", "maxHandoffs"], json!(if x == 0 { 1 } else { x })); }
            "max-minutes" => { let x = clamp_num(&v2, 1, 10080); set_in(st, &["caps", "maxMinutes"], json!(if x == 0 { 1 } else { x })); }
            "tick" => { let x = clamp_num(&v2, 0, 1440); st["tickMinutes"] = json!(if x > 0 { x.clamp(5, 1440) } else { 0 }); }
            "l5h" => { let x = clamp_num(&v2, 50, 100); set_in(st, &["limits", "pauseAt5h"], json!(if x == 0 { 100 } else { x })); }
            "lweek" => { let x = clamp_num(&v2, 50, 100); set_in(st, &["limits", "pauseAtWeek"], json!(if x == 0 { 100 } else { x })); }
            "lpar" => set_in(st, &["limits", "maxParallel"], json!(clamp_num(&v2, 0, 20))),
            "lone" => set_in(st, &["limits", "oneAtATimeAbove"], json!(clamp_num(&v2, 0, 100))),
            x if x.starts_with("preset:") => {
                let key = &x["preset:".len()..];
                let mut p = str_list(&st["presets"]);
                if v2 == "1" { if !p.iter().any(|y| y == key) { p.push(key.into()); } } else { p.retain(|y| y != key); }
                st["presets"] = json!(p);
            }
            x if x.starts_with("secret:") => {
                let id = &x["secret:".len()..];
                let mut p = str_list(&st["secrets"]);
                if v2 == "1" { if !p.iter().any(|y| y == id) { p.push(id.into()); } } else { p.retain(|y| y != id); }
                st["secrets"] = json!(p);
            }
            x if x.starts_with("cr-") => {
                let mut t = arr(st, "crTemplate").to_vec();
                let lim = n(&data()["crLimits"], "label").max(1.0) as usize;
                let hlim = n(&data()["crLimits"], "hint").max(1.0) as usize;
                if x.starts_with("cr-label:") { let i = i_of(x); if let Some(sc) = t.get_mut(i) { sc["label"] = json!(v2.chars().take(lim).collect::<String>()); } }
                else if x.starts_with("cr-hint:") { let i = i_of(x); if let Some(sc) = t.get_mut(i) { sc["hint"] = json!(v2.chars().take(hlim).collect::<String>()); } }
                else if x.starts_with("cr-required:") { let i = i_of(x); if let Some(sc) = t.get_mut(i) { sc["required"] = json!(v2 == "1"); } }
                else if x.starts_with("cr-list:") { let i = i_of(x); if let Some(sc) = t.get_mut(i) { sc["list"] = json!(v2 == "1"); } }
                else if x == "cr-up" { let i: usize = v2.parse().unwrap_or(0); if i > 0 && i < t.len() { t.swap(i, i - 1); } }
                else if x == "cr-down" { let i: usize = v2.parse().unwrap_or(0); if i + 1 < t.len() { t.swap(i, i + 1); } }
                else if x == "cr-remove" { let i: usize = v2.parse().unwrap_or(0); if t.len() > 1 && i < t.len() { t.remove(i); } }
                else if x == "cr-add" { t.push(json!({ "id": model::gen_id("s"), "label": "New section", "hint": "", "required": false, "list": false })); }
                else if x == "cr-default" { t = arr(&data(), "crTemplate").to_vec(); }
                st["crTemplate"] = Value::Array(t);
            }
            _ => {}
        }
    });
}

pub fn props() -> Value {
    with(|e| {
        let ed = &e.ed;
        json!({
            "state": ed.state, "error": ed.error, "name": ed.name, "save": ed.save, "saveLabel": ed.save_label, "draft": ed.draft,
            "canUndo": ed.can_undo, "canRedo": ed.can_redo, "tab": ed.tab, "issuesLabel": ed.issues_label, "toast": ed.toast, "zoom": e.z, "conn": ed.conn,
            "id": e.id, "sel": e.sel.clone().map(|x| format!("{}:{}", x.0, x.1)).unwrap_or_default(),
            "nodes": e.nodes.iter().map(|n2| json!({ "id": n2.id, "kind": n2.kind, "title": n2.title, "body": n2.body, "x": n2.x, "y": n2.y, "sel": n2.sel, "badge": n2.badge, "outs": n2.outs.iter().map(|o| json!({ "id": o.id, "label": o.label })).collect::<Vec<_>>() })).collect::<Vec<_>>(),
            "edges": e.edges.iter().map(|x| json!({ "id": x.id, "tone": x.tone, "label": x.label, "sel": x.sel })).collect::<Vec<_>>(),
            "people": e.people.iter().map(|p| p.label.clone()).collect::<Vec<_>>(),
            "pane": e.insp.as_ref().map(|d| json!({ "mode": d.mode, "kindLabel": d.kind_label, "label": d.label, "name": d.name, "issues": d.issues.iter().map(|i| i.message.clone()).collect::<Vec<_>>(),
                "people": d.people.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), "dkind": d.dkind_i, "options": d.options.iter().map(|o| o.label.clone()).collect::<Vec<_>>(), "rounds": d.rounds,
                "issuesTitle": d.issues_title, "persona": d.persona, "edgeDesc": d.edge_desc })).unwrap_or(Value::Null),
        })
    })
}

// ─── Drawing ───

fn calls() -> Rc<parts::CanvasCalls> {
    Rc::new(parts::CanvasCalls {
        size: Box::new(|w, h, _| size(w, h)),
        pan: Box::new(|dx, dy, _| pan(dx, dy)),
        zoom: Box::new(|d, x, y, _| zoom_at(d, x, y)),
        control: Box::new(|k, _| control(k)),
        mini: Box::new(|x, y, _| mini(x, y)),
        pane: Box::new(|_| pane_click()),
        node_down: Box::new(|id, _| node_down(id)),
        node_drag: Box::new(|id, dx, dy, _| node_drag(id, dx, dy)),
        node_up: Box::new(|id, moved, cx| node_up(id, moved, cx)),
        handle_drag: Box::new(|nd, h, x, y, _| handle_drag(nd, h, x, y)),
        handle_up: Box::new(|nd, h, x, y, cx| handle_up(nd, h, x, y, cx)),
        edge: Box::new(|id, _| edge_click(id)),
        key: Box::new(|k, cx| key(k, cx)),
    })
}

fn a(what: &'static str, x: String) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static { move |_, _, cx| fact(what, &x, "", cx) }
fn a0(what: &'static str) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static { a(what, String::new()) }
/// An edit of the pane: the field's key and value.
fn set(k: &str, v: &str, cx: &mut App) { edit_field("ed", k, v, cx); }
fn tick(id: impl Into<ElementId>, label: &str, on: bool, enabled: bool, k: String) -> impl IntoElement {
    let c = w::check(id, label.to_string(), on, move |v, _, cx| set(&k, if *v { "1" } else { "0" }, cx));
    gpui_component::Disableable::disabled(c, !enabled)
}
fn pick(id: impl Into<ElementId>, items: Vec<String>, on: usize, k: &'static str) -> impl IntoElement {
    w::dropdown_of(id, items.into_iter().map(SharedString::from).collect(), on, false, true, move |i, _, cx| set(k, &i.to_string(), cx))
}
fn line(k: &str, v: &str, ph: &str, mono: bool, window: &mut Window, cx: &mut App) -> AnyElement {
    let k2 = k.to_string();
    parts::line(&format!("ed|{k}"), v, ph, mono, false, parts::Commit::Change, window, cx, move |t, cx| set(&k2, &t, cx))
}
fn area(k: &str, v: &str, ph: &str, rows: usize, mono: bool, window: &mut Window, cx: &mut App) -> AnyElement {
    let k2 = k.to_string();
    parts::area(&format!("ed|{k}"), v, ph, rows, mono, window, cx, move |t, cx| set(&k2, &t, cx))
}

/// The pane's title row (.ff-pane-title): back, the words, delete.
fn pane_title(text: &str, back: bool, del: bool, del_tip: &'static str, on_del: &'static str) -> Div {
    div().flex().items_center().gap(T::SP_8)
        .when(back, |d| d.child(w::icon_button("e-team-back", IconName::ArrowLeft, "Back to the team").on_click(a0("e-team"))))
        .child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).child(text.to_string()))
        .when(del, |d| d.child(w::icon_button("e-del", IconName::Trash, del_tip).on_click(a0(on_del))))
}

fn issues(d: Stateful<Div>, list: &[Issue], pick: bool) -> Stateful<Div> {
    let mut d = d;
    for (i, it) in list.iter().enumerate() {
        let k = it.key.clone();
        let el = parts::issue(("e-issue", i), &it.level, &it.message, &it.fix);
        d = d.child(if pick && !k.is_empty() { el.cursor_pointer().on_click(a("e-issue", k)) } else { el });
    }
    d
}

fn choices(key: &'static str, items: &[Opt], min: usize, add: &str, window: &mut Window, cx: &mut App) -> Div {
    let mut d = div().flex().flex_col().gap(T::SP_6);
    for (i, it) in items.iter().enumerate() {
        let (id, id2) = (it.id.clone(), it.id.clone());
        let k = format!("{key}-label:{id}");
        d = d.child(div().flex().items_center().gap(T::SP_4)
            .child(div().flex_1().min_w_0().child(line(&k, &it.label, &format!("Choice {}", i + 1), false, window, cx)))
            .child(w::disabled(w::icon_button((key, i), IconName::X, "Remove"), items.len() <= min).on_click(move |_, _, cx| set(&format!("{key}-remove"), &id2, cx))));
        let _ = id;
    }
    d.child(div().child(btn((key, 999usize), add, Some(IconName::Plus), "ghost", true, false).on_click(move |_, _, cx| set(&format!("{key}-add"), "", cx))))
}

fn checks(key: &'static str, items: &[Check]) -> Div {
    let mut d = div().flex().flex_col().gap(T::SP_6);
    for (i, c) in items.iter().enumerate() {
        d = d.child(tick((key, i), &format!("{}{}", c.label, if c.sub.is_empty() { String::new() } else { format!(" · {}", c.sub) }), c.on, c.enabled, format!("{key}:{}", c.id)));
    }
    d
}

fn node_insp(d: &Insp, p: Stateful<Div>, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    let mut p = issues(p.child(pane_title(&d.kind_label, false, d.can_delete, "Delete", "e-delete")), &d.issues, false);
    p = p.child(field("Label", "", "Shown on the canvas; also how the team refers to this step.", line("label", &d.label, &d.label_ph, false, window, cx)));
    match d.kind.as_str() {
        "employee" => {
            p = p.child(field("Who does this step", "", "", pick("e-who", d.who.clone(), d.who_i, "who")));
            if d.has_person { p = p.child(div().child(btn("e-persona", "Edit their persona", Some(IconName::User), "ghost", true, false).on_click(a0("e-person")))); }
        }
        "manager" => { p = p.child(div().child(btn("e-mpersona", "Edit the Manager's persona", Some(IconName::Crown), "ghost", true, false).on_click(a("e-person", "manager".into())))); }
        "decision" => {
            p = p.child(field("Question", "", "Answered at handoff. You can name skills with /skill-name.", div().flex().flex_col().gap(T::SP_4).child(area("question", &d.question, "Does it need a UI change?", 2, false, window, cx)).child(parts::mentions(&d.mentions_q))))
                .child(field("Kind of answer", "", "", pick("e-dkind", vec!["Yes or no".into(), "Pick one".into(), "Pick any (runs them in parallel)".into()], d.dkind_i, "dkind")));
            if d.dkind_i != 0 { p = p.child(field("Options", "", "", choices("options", &d.options, 2, "Add option", window, cx))); }
            if d.dkind_i == 0 {
                p = p.child(tick("e-review", "This is a review gate (approval)", d.review, true, "review".into()));
                if d.review { p = p.child(note("hint", "\"No\" needs at least one open blocking finding saying what must change; \"yes\" needs none left open. Findings carry over between rounds, so reviewers check last round's requests first, and nits become follow-ups instead of blocking.")); }
            }
            p = p.child(field("Who answers", "", "Usually whoever arrives here. After a Join or an Ask me, someone has to be named.", pick("e-answered", d.answered.clone(), d.answered_i, "answered")));
        }
        "ask" => {
            p = p.child(field("What you are asked", "", "You get a notification and the run waits for your reply.", div().flex().flex_col().gap(T::SP_4).child(area("prompt", &d.prompt, "Ship it?", 2, false, window, cx)).child(parts::mentions(&d.mentions_q))))
                .child(field("Replies", "", "", choices("replies", &d.replies, 1, "Add reply", window, cx)));
        }
        "wait" => {
            p = p.child(field("Wait for", "", "", pick("e-wkind", vec!["A command to succeed (CI, a deploy, a health check)".into(), "A signal (a URL called by CI or a hook, or you)".into(), "A length of time".into(), "A time of day".into()], d.wkind_i, "wkind")));
            if d.wkind_i == 2 { p = p.child(field("Minutes", "", "", line("minutes", &d.minutes, "", false, window, cx))); }
            if d.wkind_i == 3 { p = p.child(field("Until (this server's time)", "", "The next time the clock says this; tomorrow if it has passed today.", line("wat", &d.at, "07:30", false, window, cx))); }
            if d.wkind_i == 0 {
                p = p.child(field("Command", "", "Run in the run's worktree; the wait is over when it exits 0. E.g. gh run watch --exit-status, curl -fs localhost:4310/health, npm test.", line("command", &d.command, "gh run watch --exit-status", true, window, cx)))
                    .child(field("Try every (seconds)", "", "", line("every", &d.every, "", false, window, cx)));
            }
            if d.wkind_i == 1 { p = p.child(field("Waiting for", "", "Shown on the live graph. Each run gets a secret URL to POST to; you can also end the wait from the run.", line("what", &d.what, "The staging deploy to finish", false, window, cx))); }
            if d.wkind_i <= 1 { p = p.child(field("Give up after (minutes)", "", "Then it goes down Timed out, so the run never waits forever.", line("timeout", &d.timeout, "", false, window, cx))); }
            p = p.child(note("hint", "What came in (the last summary, any note) goes on to whoever is next, with what happened during the wait. Waits survive a restart."));
        }
        "join" => { p = p.child(field("Joins the branches of", "", "The pick-any decision whose parallel branches meet here.", pick("e-fork", d.forks.clone(), d.fork_i, "fork"))); }
        "start" => { p = p.child(note("hint", "The task you give when starting the factory enters here and goes to the Manager.")); }
        "end" => { p = p.child(note("hint", "Only the Manager's Finish reaches here. The Manager then presents the result and you decide whether to merge it.")); }
        _ => {}
    }
    p
}

fn edge_insp(d: &Insp, p: Stateful<Div>, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    let mut p = p.child(pane_title(if d.note { "Note" } else { "Connection" }, false, d.can_delete, "Delete", "e-delete")).child(note("hint", d.edge_desc.clone()));
    p = issues(p, &d.issues, false);
    if d.note {
        p = p.child(note("hint", d.label.clone())).child(field("What the note is for", "", "", line("note-label", &d.note_label, "Instructions for reviewers", false, window, cx)));
    } else {
        p = p.child(field("Round limit", "", "Set it on the connection that goes back round a loop. When it runs out, the Manager takes over. Empty means no limit.", line("rounds", &d.rounds, "No limit", false, window, cx)))
            .child(tick("e-reqnote", "Require a note on this route", d.require_note, true, "require-note".into()));
        if d.require_note { p = p.child(field("What the note says", "", "", line("note-label", &d.note_label, "What to change", false, window, cx))); }
    }
    p
}

fn pair(a: impl IntoElement, b: impl IntoElement) -> Div { div().flex().gap(T::SP_8).child(div().flex_1().min_w_0().child(a)).child(div().flex_1().min_w_0().child(b)) }

fn settings_insp(d: &Insp, p: Stateful<Div>, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    let cr_open = with(|e| e.cr_open);
    let mut p = p.child(pane_title("Factory", false, false, "", ""))
        .child(field("Name", "", "", line("name", &d.name, "", false, window, cx)))
        .child(field("Description", "", "What this factory is for. /skill-name works here too.", div().flex().flex_col().gap(T::SP_4).child(area("description", &d.description, "", 3, false, window, cx)).child(parts::mentions(&d.mentions_d))))
        .child(w::disclosure("e-cr-open", "Change request form", cr_open).on_click(a0("e-cr-open")));
    if cr_open {
        let mut c = div().flex().flex_col().gap(T::SP_8).child(note("hint", "What the person fills in to start a run. The Manager gets all of it; every step gets the title, the goal and the \"done\" checks. The title and attachments are always there."));
        let total = d.cr.len();
        for (i, sec) in d.cr.iter().enumerate() {
            let is = i.to_string();
            let (i1, i2, i3) = (is.clone(), is.clone(), is.clone());
            c = c.child(div().flex().flex_col().gap(T::SP_6).p(T::SP_8).rounded(T::R_FIELD).border_1().border_color(T::border()).bg(T::bg_card())
                .child(div().flex().items_center().gap(T::SP_4)
                    .child(div().flex_1().min_w_0().child(line(&format!("cr-label:{i}"), &sec.label, "Section name", false, window, cx)))
                    .child(w::disabled(w::icon_button(("e-cr-up", i), IconName::ChevronUp, "Move up"), i == 0).on_click(move |_, _, cx| set("cr-up", &i1, cx)))
                    .child(w::disabled(w::icon_button(("e-cr-down", i), IconName::ChevronDown, "Move down"), i + 1 >= total).on_click(move |_, _, cx| set("cr-down", &i2, cx)))
                    .child(w::disabled(w::icon_button(("e-cr-rm", i), IconName::Trash, "Remove"), total <= 1).on_click(move |_, _, cx| set("cr-remove", &i3, cx))))
                .child(line(&format!("cr-hint:{i}"), &sec.hint, "Hint: what to write here", false, window, cx))
                .child(div().flex().gap(T::SP_16)
                    .child(tick(("e-cr-req", i), "Required", sec.required, true, format!("cr-required:{i}")))
                    .child(tick(("e-cr-list", i), "A list", sec.list, true, format!("cr-list:{i}")))));
        }
        c = c.child(div().flex().gap(T::SP_6)
            .child(btn("e-cr-add", "Add a section", Some(IconName::Plus), "", !d.cr_full, false).on_click(|_, _, cx| set("cr-add", "", cx)))
            .when(!d.cr_default, |x| x.child(btn("e-cr-default", "Back to the default", None, "ghost", true, false).on_click(|_, _, cx| set("cr-default", "", cx)))));
        p = p.child(c);
    }
    p = p.child(field("Manager's permissions", "", &d.mmode_hint, pick("e-mmode", vec!["Normal: ask me for the Manager's own actions".into(), "Bypass: I trust the Manager".into()], d.mmode_i, "mmode")))
        .child(field("The team's permissions", "", &d.tmode_hint, pick("e-tmode", vec![if d.gatekeeper { "Ask the Manager (through the Gatekeeper)".into() } else { "Ask the Manager".into() }, "Bypass too".into()], d.tmode_i, "tmode")));
    if d.bypass_warn {
        p = p.child(div().flex().gap(T::SP_8).p(T::SP_8).rounded(T::R_CONTROL).bg(T::yellow_dim())
            .child(div().pt(T::SP_2).text_color(T::yellow()).child(w::icon(IconName::TriangleAlert, T::ICON_MD)))
            .child(div().flex().flex_col().flex_1().gap(T::SP_6)
                .child(div().text_size(T::TEXT_12).text_color(T::yellow()).child("You trust the Manager with bypass, and it would approve most of its team's requests anyway. Giving the team bypass too saves every one of those round trips."))
                .child(div().child(btn("e-bypass-too", "Give the team bypass too", None, "", true, false).on_click(|_, _, cx| set("tmode", "1", cx))))));
    }
    if d.tmode_i == 0 {
        p = p.child(field("Everyone may, without asking", "", "Each person can have more in the Team panel.", checks("preset", &d.presets)))
            .child(field("More rules for everyone", "", "One per line, Claude Code style: Bash(docker compose up:*), WebFetch(domain:docs.example.com). Too broad or forbidden rules are ignored.", area("rules", &d.rules, "", 3, true, window, cx)))
            .child(tick("e-mrules", "The Manager may set standing rules", d.manager_rules, true, "manager-rules".into()))
            .child(note("hint", d.mrules_hint.clone()))
            .child(tick("e-gk", "A Gatekeeper answers requests first", d.gatekeeper, true, "gatekeeper".into()));
        if d.gatekeeper { p = p.child(field("Gatekeeper model", "", "A small, cheap model is enough: it follows the Manager's policy and passes the unclear requests on, so the Manager's own conversation stays on the work.", pick("e-gkm", vec!["Haiku".into(), "Sonnet".into(), "Opus".into()], d.gk_i, "gk"))); }
    }
    if d.secrets_on {
        let mut c = div().flex().flex_col().gap(T::SP_6);
        if d.secrets.is_empty() { c = c.child(note("hint", "No secrets on this machine yet. Add them under Secrets.")); }
        c = c.child(checks("secret", &d.secrets));
        for (i, g) in d.secrets_gone.iter().enumerate() {
            let k = format!("secret:{g}");
            c = c.child(div().flex().flex_col().gap(T::SP_6).p(T::SP_8).rounded(T::R_CONTROL).bg(T::yellow_dim())
                .child(div().text_size(T::TEXT_12).text_color(T::yellow()).child("A secret this factory lists no longer exists."))
                .child(div().child(btn(("e-secret-gone", i), "Remove it", None, "", true, false).on_click(move |_, _, cx| set(&k, "0", cx)))));
        }
        p = p.child(field("Secrets the run may use", "", "Their PINs are asked when you start a run and kept in memory for that run only, so it does not stop at every use. Values are never shown. The Manager and the Gatekeeper use them directly; anyone else asks them first. Any other secret asks you each time.", c));
    }
    p.child(field("When the work is done", "", "Either way it is a single squash commit, and nothing is ever pushed.", pick("e-merge", vec!["Show me first, I decide".into(), "Merge automatically (with Undo)".into()], d.merge_i, "merge")))
        .child(tick("e-autorec", "Recover by itself after a crash", d.auto_recover, true, "auto-recover".into()))
        .child(field("Setup command", "", "Runs in each new worktree before the team starts, e.g. npm ci.", line("setup", &d.setup, "npm ci", true, window, cx)))
        .child(pair(field("Max handoffs", "", "", line("max-handoffs", &d.max_handoffs, "", false, window, cx)), field("Max minutes", "", "", line("max-minutes", &d.max_minutes, "", false, window, cx))))
        .child(note("hint", "When a limit is reached the run waits for you; you can let it carry on."))
        .child(field("Manager check-in every (minutes)", "", &d.tick_hint, line("tick", &d.tick, "The Manager chooses", false, window, cx)))
        .child(sub("USAGE"))
        .child(note("hint", "Your Claude usage is shared with your own chats. These keep a run inside what you choose; each can also be changed on a running run."))
        .child(pair(field("5-hour, pause at %", "", "", line("l5h", &d.l5h, "", false, window, cx)), field("Weekly, pause at %", "", "", line("lweek", &d.lweek, "", false, window, cx))))
        .child(note("hint", d.lim_hint.clone()))
        .child(pair(field("People at once", "", "0: no limit", line("lpar", &d.lpar, "", false, window, cx)), field("One at a time above %", "", "Of the 5-hour. 0: off", line("lone", &d.lone, "", false, window, cx))))
        .child(note("hint", "Parallel steps beyond the limit wait their turn instead of all running together, which spreads the load. Each person's model is in the Team panel: a cheaper model for routine roles stretches usage too."))
}

fn team_insp(d: &Insp, p: Stateful<Div>) -> Stateful<Div> {
    let mut list = div().flex().flex_col().gap(T::SP_4);
    for (i, pp) in d.people.iter().enumerate() {
        list = list.child(div().id(("e-person", i)).flex().items_center().gap(T::SP_8).px(T::SP_10).py(T::SP_8).rounded(T::R_FIELD).border_1().border_color(T::border())
            .bg(T::bg_card()).hover(|d| d.bg(T::bg_hover())).cursor_pointer()
            .child(div().text_color(T::text_secondary()).child(w::icon(if pp.manager { IconName::Crown } else { IconName::User }, T::ICON_LG)))
            .child(div().flex().flex_col().flex_1().min_w_0()
                .child(div().truncate().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).child(pp.name.clone()))
                .child(div().truncate().text_size(T::TEXT_11).text_color(T::text_muted()).child(pp.meta.clone())))
            .on_click(a("e-person", pp.id.clone())));
    }
    let mut pills = div().flex().flex_wrap().gap(T::SP_6);
    for (i, o) in d.personas.iter().enumerate() {
        pills = pills.child(btn(("e-tpl", i), &o.label, Some(IconName::Plus), "", true, false).on_click(a("e-add-person", o.id.clone())));
    }
    p.child(pane_title("Team", false, false, "", "")).child(list).child(sub("ADD SOMEONE")).child(pills)
        .child(note("hint", "Templates are starting points: change the persona however you like."))
}

fn person_insp(d: &Insp, p: Stateful<Div>, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    let more = with(|e| e.more_open);
    let mut p = issues(p.child(pane_title(if d.manager { "Manager" } else { "Employee" }, true, !d.manager, "Remove from the team", "e-remove-person")), &d.issues, false);
    p = p.child(field("Name", "", "", line("p-name", &d.name, "", false, window, cx)))
        .child(field("Persona", "", "Who they are and how they work. They read it and act as it says. /skill-name names a skill they should use.", div().flex().flex_col().gap(T::SP_4).child(area("persona", &d.persona, "", 9, false, window, cx)).child(parts::mentions(&d.mentions_p))))
        .child(pair(field("Model", "", "", pick("e-model", vec!["Default".into(), "Opus".into(), "Sonnet".into(), "Haiku".into()], d.model_i, "model")),
            field("Effort", "", "", pick("e-effort", ["Default", "low", "medium", "high", "xhigh", "max"].iter().map(|x| x.to_string()).collect(), d.effort_i, "effort"))));
    if !d.manager {
        p = p.child(field("Memory", "", &d.memory_hint, pick("e-memory", vec!["Keeps memory".into(), "Fresh each time".into()], d.memory_i, "memory")))
            .child(tick("e-canedit", "Can edit files", d.can_edit, true, "can-edit".into()));
    }
    p = p.child(field("Pinned skills", "", "Comma separated. Same as naming them in the persona.", line("pinned", &d.pinned, "frontend-design, run", false, window, cx)));
    if d.show_presets { p = p.child(field("This person may also, without asking", "", "On top of what everyone may (factory settings).", checks("p-preset", &d.p_presets))); }
    p = p.child(w::disclosure("e-more", "More tool rules", more).on_click(a0("e-more")));
    if more {
        p = p.child(field("Always allowed (this person only)", "", "Claude Code rules, comma separated, e.g. Bash(npm test:*).", line("allow", &d.allow, "", true, window, cx)))
            .child(field("Never allowed", "", "", line("deny", &d.deny, "", true, window, cx)));
    }
    if !d.manager { p = p.child(note("hint", d.steps_hint.clone())); }
    p
}

fn issues_insp(d: &Insp, p: Stateful<Div>) -> Stateful<Div> {
    let mut p = p.child(pane_title(&d.issues_title, false, false, "", ""));
    if d.ok && d.issues.is_empty() {
        p = p.child(div().flex().items_center().gap(T::SP_6).text_color(T::green()).text_size(T::TEXT_13)
            .child(w::icon(IconName::Check, T::ICON_MD)).child("Every path can finish, every loop has a limit, every answer is wired."));
    }
    if !d.issues_hint.is_empty() { p = p.child(note("hint", d.issues_hint.clone())); }
    issues(p, &d.issues, true)
}

fn dry_insp(d: &Insp, p: Stateful<Div>) -> Stateful<Div> {
    p.child(pane_title("Dry run", false, false, "", ""))
        .child(note("hint", "Walk through the flow by hand, answering for each person, before spending anything on Claude. The canvas shows where the work is and how many rounds each loop has used."))
        .when(!d.ok, |p| p.child(parts::issue("e-dry-bad", "error", "Fix the problems first: a factory that cannot run cannot be walked either.", "")))
        .child(div().child(btn("e-dry", "Start a dry run", Some(IconName::Play), "primary", false, false)))
        .child(note("hint", "The dry run is in the web app for now: it walks the flow with the server's factory engine, which this client does not have yet."))
}

/// A palette item (.ff-pal-item): a click adds it in the middle of the view; dragged onto the canvas, where it is let go.
#[derive(Clone)]
struct PalDrag { kind: String, label: String }
struct PalGhost(String);
impl Render for PalGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().px(T::SP_10).h(T::CONTROL_SM).flex().items_center().rounded(T::R_FIELD).bg(T::bg_card()).border_1().border_color(T::accent()).text_size(T::TEXT_12).child(self.0.clone())
    }
}

fn pal_item(i: usize, label: &str, icon: IconName, ghost: bool, kind: String) -> Stateful<Div> {
    let k2 = kind.clone();
    div().id(("e-pal", i)).flex().items_center().gap(T::SP_8).px(T::SP_8).h(T::CONTROL_SM + T::SP_2).rounded(T::R_FIELD).border_1().border_color(T::border())
        .when(!ghost, |d| d.bg(T::bg_card())).hover(|d| d.border_color(T::accent())).cursor_grab()
        .text_color(if ghost { T::text_secondary() } else { T::text() })
        .child(w::icon(icon, T::ICON_MD)).child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12).child(label.to_string()))
        .on_click(move |_, _, cx| fact("e-add", &k2, "", cx))
        .on_drag(PalDrag { kind, label: label.to_string() }, |d, _, _, cx| cx.new(|_| PalGhost(d.label.clone())))
}

/// The editor.
pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let (ed, nodes, edges, people, insp) = with(|e| (e.ed.clone(), e.nodes.clone(), e.edges.clone(), e.people.clone(), e.insp.clone()));
    if ed.state == "loading" || ed.state.is_empty() { return div().p(T::SP_24).child(note("note", "Loading…")).into_any_element(); }
    if ed.state == "error" {
        return div().flex().flex_col().p(T::SP_24).gap(T::SP_10).child(note("note", ed.error.clone()))
            .child(div().child(btn("e-back-err", "Back", None, "", true, false).on_click(a0("e-back")))).into_any_element();
    }
    let narrow = parts::bounds_of("fx-ed").map(|b| b.size.width < T::FACTORIES_NARROW).unwrap_or(false);
    let save_c = if ed.save == "conflict" || ed.save == "error" { T::red() } else if ed.draft { T::yellow() } else if ed.save == "saving" || ed.save == "dirty" { T::text_secondary() } else { T::text_muted() };
    let top = div().flex().flex_none().items_center().gap(T::SP_6).px(T::SP_8).h(T::CONTROL + T::SP_10).bg(T::bg_secondary()).border_b_1().border_color(T::border())
        .child(w::icon_button("e-back", IconName::ArrowLeft, "Back").on_click(a0("e-back")))
        .child(div().id("e-name").px(T::SP_6).rounded(T::R_CONTROL).cursor_pointer().hover(|d| d.bg(T::bg_hover())).max_w(T::FACTORIES_RIGHT)
            .truncate().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).child(if ed.name.is_empty() { "Untitled factory".to_string() } else { ed.name.clone() })
            .on_click(a0("e-settings")))
        .child(div().text_size(T::TEXT_11).text_color(save_c).child(format!("{}{}", ed.save_label, if ed.draft { " · draft" } else { "" })))
        .when(ed.save == "conflict", |d| d.child(btn("e-reload", "Reload", None, "", true, false).on_click(a0("e-reload"))))
        .when(ed.save == "error", |d| d.child(btn("e-retry", "Retry", None, "", true, false).on_click(a0("e-save"))))
        .child(div().flex_1())
        .child(w::disabled(w::icon_button("e-undo", IconName::Undo2, "Undo (Ctrl+Z)"), !ed.can_undo).on_click(a0("e-undo")))
        .child(w::disabled(w::icon_button("e-redo", IconName::Redo2, "Redo (Ctrl+Shift+Z)"), !ed.can_redo).on_click(a0("e-redo")))
        .child(w::icon_button("e-tidy", IconName::LayoutGrid, "Tidy the layout").on_click(a0("e-tidy")))
        .child(w::icon_button("e-fit", IconName::Maximize, "Fit to screen").on_click(|_, _, cx| fact("cv-fit", "e", "", cx)));
    let mut row = div().flex().flex_1().min_h_0();
    if !narrow {
        let mut pal = div().id("e-palette").flex().flex_col().flex_none().w(T::FACTORIES_LEFT).bg(T::bg_secondary()).border_r_1().border_color(T::border()).overflow_y_scroll().p(T::SP_12).gap(T::SP_10)
            .child(sub("PEOPLE"));
        let mut i = 0;
        for p in &people { pal = pal.child(pal_item(i, &p.label, IconName::User, false, format!("employee:{}", p.id))); i += 1; }
        pal = pal.child(pal_item(i, "Someone new", IconName::Plus, true, "employee:".into())).child(sub("FLOW"));
        for (label, icon, kind) in [("Yes / no question", IconName::GitBranch, "decision:yesno"), ("Pick one", IconName::GitBranch, "decision:one"), ("Pick any (parallel)", IconName::GitBranch, "decision:many"),
            ("Join", IconName::GitMerge, "join"), ("Ask me", IconName::MessageCircleQuestionMark, "ask"), ("Wait", IconName::Clock, "wait")] {
            i += 1;
            pal = pal.child(pal_item(i, label, icon, false, kind.into()));
        }
        pal = pal.child(note("hint", "Drag onto the canvas, or tap to add. Connect an output (bottom) to a step's top. Drag from the side dot of a person to leave a note."));
        row = row.child(pal);
    }
    let cv = parts::canvas_view(parts::Canvas { which: "e", nodes, edges, zoom: ed.zoom, pan_x: ed.pan_x, pan_y: ed.pan_y, editable: true, minimap: !narrow, controls_top: narrow, bounds: (ed.bx, ed.by, ed.bw, ed.bh), conn: ed.conn.then(|| ed.conn_path.clone()), cb: calls() }, window, cx);
    row = row.child(div().relative().flex().flex_1().min_w_0()
        .on_drop(|d: &PalDrag, window, cx| {
            let (x, y) = { let o = parts::bounds_of("cv-e").map(|b| b.origin).unwrap_or_default(); let m = window.mouse_position(); (f32::from(m.x - o.x), f32::from(m.y - o.y)) };
            fact("e-add-at", &d.kind, &format!("{x},{y}"), cx);
        })
        .child(cv)
        .when(!ed.toast.is_empty(), |d| d.child(div().absolute().bottom(T::SP_16).left_0().right_0().flex().justify_center()
            .child(div().max_w(T::FACTORIES_MODAL_W).px(T::SP_12).py(T::SP_8).rounded(T::R_FIELD).border_1().border_color(T::border()).bg(T::bg_card()).text_size(T::TEXT_12).child(ed.toast.clone())))));
    // .ff-right: the tabs and the pane.
    let tab = |i: usize, id: &'static str, icon: IconName, label: String| {
        let on = ed.tab == id;
        let fg = if id == "issues" && ed.issues_bad { T::yellow() } else if on { T::text() } else { T::text_secondary() };
        div().id(("e-tab", i)).flex().flex_1().items_center().justify_center().gap(T::SP_4).h(T::CONTROL_SM + T::SP_2).rounded(T::R_CONTROL).cursor_pointer()
            .text_size(T::TEXT_12).text_color(fg).when(on, |d| d.bg(T::bg_hover())).hover(|d| d.bg(T::bg_hover()))
            .child(w::icon(icon, T::ICON_MD)).child(label)
            .on_click(move |_, _, cx| fact("e-tab", id, "", cx))
    };
    let tabs = div().flex().flex_none().gap(T::SP_2).p(T::SP_6).border_b_1().border_color(T::border())
        .child(tab(0, "edit", IconName::SlidersHorizontal, "Edit".into()))
        .child(tab(1, "team", IconName::Users, "Team".into()))
        .child(tab(2, "issues", IconName::TriangleAlert, ed.issues_label.clone()))
        .child(tab(3, "dry", IconName::Route, "Dry run".into()));
    let pane = div().id("e-pane").flex().flex_col().flex_1().min_h_0().overflow_y_scroll().p(T::SP_12).gap(T::SP_10);
    let pane = match insp.as_ref() {
        Some(d) => match d.mode.as_str() {
            "node" => node_insp(d, pane, window, cx),
            "edge" => edge_insp(d, pane, window, cx),
            "team" => team_insp(d, pane),
            "person" => person_insp(d, pane, window, cx),
            "issues" => issues_insp(d, pane),
            "dry" => dry_insp(d, pane),
            _ => settings_insp(d, pane, window, cx),
        },
        None => pane,
    };
    row = row.child(div().flex().flex_col().flex_none().w(T::FACTORIES_RIGHT).bg(T::bg_secondary()).border_l_1().border_color(T::border()).child(tabs).child(pane));
    div().relative().flex().flex_col().size_full().min_h_0()
        .child(parts::probe("fx-ed", |_, _| {}))
        .on_key_down(|e, _, cx| {
            let m = e.keystroke.modifiers;
            if !(m.control || m.platform) { return; }
            match e.keystroke.key.as_str() {
                "s" => { fact("e-save", "", "", cx); cx.stop_propagation(); }
                "z" => { fact(if m.shift { "e-redo" } else { "e-undo" }, "", "", cx); cx.stop_propagation(); }
                "y" => { fact("e-redo", "", "", cx); cx.stop_propagation(); }
                _ => {}
            }
        })
        .child(top).child(row).into_any_element()
}
