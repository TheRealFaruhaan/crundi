//! Factories (4.7): the factory editor (tools/factory-flow/src/Editor.jsx,
//! Palette, Inspector, Roster, CrTemplate). The factory stays the source of
//! truth: every change edits its JSON (undo and redo keep copies, typing
//! in one field coalesces), the canvas is drawn from it, and it is saved
//! 1.2 s after the last change (Ctrl+S at once; Saved / Unsaved / Saving… /
//! Changed elsewhere with Reload / Not saved with Retry).
//!
//! Known gaps: the web checks a factory in the browser as it changes (the
//! server's validateFactory, bundled); this client shows the server's
//! check, which comes back with each save. The Dry run walks the flow with
//! the server's engine, bundled in the web; this client has no engine.

use super::model::{self, s, n, b, arr, Rect};
use super::{g, get, post, ss, rc_of, strs, err_text};
use crate::{kit, PlanningFactories, PlanningFxNode, PlanningFxEdge, PlanningFxOut, PlanningFxOpt, PlanningFxOptRow, PlanningFxInsp, PlanningFxIssue, PlanningFxPerson, PlanningFxCheck, PlanningFxCrSec, PlanningFxEd};
use serde_json::{json, Value};
use slint::{Model, VecModel};
use std::{cell::RefCell, collections::{HashMap, HashSet}, rc::Rc};

pub struct Models {
    nodes: Rc<VecModel<PlanningFxNode>>,
    edges: Rc<VecModel<PlanningFxEdge>>,
    people: Rc<VecModel<PlanningFxOpt>>,
    insp: Rc<VecModel<PlanningFxInsp>>,
}
impl Models {
    pub fn new(f: &PlanningFactories) -> Models {
        let m = Models { nodes: kit::model(), edges: kit::model(), people: kit::model(), insp: kit::model() };
        f.set_e_nodes(kit::rc(&m.nodes)); f.set_e_edges(kit::rc(&m.edges)); f.set_e_people(kit::rc(&m.people)); f.set_e_insp(kit::rc(&m.insp));
        m
    }
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
    insp_gen: i32,
    state: String,
    error: String,
    rects: HashMap<String, Rect>,
    outs: HashMap<String, Vec<String>>,
    gen: u64,
}

thread_local! { static ES: RefCell<E> = RefCell::new(E { z: 1.0, ..Default::default() }); }
fn with<R>(f: impl FnOnce(&mut E) -> R) -> R { ES.with(|s| f(&mut s.borrow_mut())) }

fn data() -> Value { serde_json::from_str(super::data::DATA).unwrap_or(Value::Null) }
const EDIT_TOOLS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

pub fn reset() {
    let (open, dirty) = with(|e| (e.open, e.dirty));
    if open && dirty { save_now(|_| {}); }
    with(|e| { let (cw, ch) = (e.cw, e.ch); *e = E { z: 1.0, cw, ch, ..Default::default() }; });
}
pub fn zoom() -> f32 { with(|e| e.z) }

/// mountEditor(): load the factory, then the skills and the secrets.
pub fn open(id: &str) {
    let gen = with(|e| { let (cw, ch, g2) = (e.cw, e.ch, e.gen + 1); *e = E { open: true, id: id.to_string(), z: 1.0, cw, ch, tab: "edit".into(), save: "saved".into(), state: "loading".into(), gen: g2, ..Default::default() }; g2 });
    push_ed();
    super::models(|m| { m.e.nodes.set_vec(vec![]); m.e.edges.set_vec(vec![]); m.e.insp.set_vec(vec![]); });
    let path = format!("/api/factories/get?id={}", kit::enc(id));
    get(path, move |j| {
        if with(|e| e.gen != gen) { return; }
        if !b(&j, "ok") {
            let er = err_text(&j, "Could not open this factory.");
            with(|e| { e.state = "error".into(); e.error = er; });
            push_ed();
            return;
        }
        with(|e| { e.f = j["factory"].clone(); e.version = n(&j["factory"], "version") as i64; e.validation = j["validation"].clone(); e.state = "ok".into(); });
        render();
    });
    get("/api/skills".into(), move |j| {
        if with(|e| e.gen != gen) { return; }
        let list: Vec<(String, String, String)> = arr(&j, "skills").iter().map(|x| (s(x, "name"), s(x, "description"), s(x, "scope"))).collect();
        with(|e| e.skills = list);
        render();
    });
    super::caps(move |c| {
        if !c.iter().any(|x| x == "factory-secrets") { return; }
        get("/api/secrets".into(), move |j| {
            if with(|e| e.gen != gen) || !b(&j, "ok") { return; }
            with(|e| e.secrets = Some(arr(&j, "secrets").to_vec()));
            render();
        });
    });
}

// ─── Changes, undo, redo ───

fn mutate(key: &str, f: impl FnOnce(&mut Value)) {
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
    if ok { schedule_save(); render(); }
}

fn undo() {
    let ok = with(|e| { let Some(p) = e.past.pop() else { return false }; let cur = std::mem::replace(&mut e.f, p); e.future.push(cur); e.last_key.clear(); e.dirty = true; e.save = "dirty".into(); true });
    if ok { schedule_save(); render(); }
}
fn redo() {
    let ok = with(|e| { let Some(p) = e.future.pop() else { return false }; let cur = std::mem::replace(&mut e.f, p); e.past.push(cur); e.last_key.clear(); e.dirty = true; e.save = "dirty".into(); true });
    if ok { schedule_save(); render(); }
}

// ─── Saving ───

fn schedule_save() {
    let gen = with(|e| { e.save_gen += 1; e.save_gen });
    slint::Timer::single_shot(std::time::Duration::from_millis(1200), move || { if with(|e| e.save_gen == gen) { save_now(|_| {}); } });
}

/// doSave(): true when saved (or nothing to save).
fn save_now(then: impl FnOnce(bool) + 'static) {
    let job = with(|e| {
        e.save_gen += 1;
        if !e.dirty || !e.open { return None; }
        e.dirty = false; e.save = "saving".into();
        Some((e.id.clone(), e.f.clone(), e.version, e.gen))
    });
    let Some((id, f, version, gen)) = job else { then(true); return };
    push_ed();
    let k = super::super::park(move |r| {
        let saved = if with(|e| e.gen != gen) { false } else if b(&r, "ok") {
            with(|e| { e.version = n(&r["factory"], "version") as i64; if r.get("validation").is_some() { e.validation = r["validation"].clone(); } e.save = if e.dirty { "dirty".into() } else { "saved".into() }; });
            true
        } else if b(&r, "conflict") { with(|e| e.save = "conflict".into()); false } else { with(|e| { e.dirty = true; e.save = "error".into(); }); false };
        render();
        then(saved);
    });
    post("/api/factories", json!({ "action": "save", "id": id, "definition": f, "baseVersion": version }), move |r| super::super::unpark(k, r));
}

fn reload() {
    let (id, gen) = with(|e| (e.id.clone(), e.gen));
    get(format!("/api/factories/get?id={}", kit::enc(&id)), move |j| {
        if with(|e| e.gen != gen) || !b(&j, "ok") { return; }
        with(|e| { e.f = j["factory"].clone(); e.version = n(&j["factory"], "version") as i64; e.validation = j["validation"].clone(); e.past.clear(); e.future.clear(); e.dirty = false; e.save = "saved".into(); });
        render();
    });
}

fn flash(t: &str) {
    let gen = with(|e| { e.toast = t.to_string(); e.toast_gen += 1; e.toast_gen });
    push_ed();
    slint::Timer::single_shot(std::time::Duration::from_millis(4000), move || { if with(|e| e.toast_gen == gen) { with(|e| e.toast.clear()); push_ed(); } });
}

// ─── Drawing ───

fn push_ed() {
    let ed = with(|e| {
        let ok = b(&e.validation, "ok");
        let errs = arr(&e.validation, "errors").len();
        PlanningFxEd {
            state: ss(&e.state), error: ss(&e.error), name: ss(s(&e.f, "name")), save: ss(&e.save),
            save_label: ss(match e.save.as_str() { "saving" => "Saving…", "dirty" => "Unsaved", "conflict" => "Changed elsewhere", "error" => "Not saved", _ => "Saved" }),
            draft: e.state == "ok" && !ok, can_undo: !e.past.is_empty(), can_redo: !e.future.is_empty(), tab: ss(&e.tab),
            issues_label: ss(if ok { "Issues".to_string() } else { format!("Fix {errs}") }), issues_bad: !ok && e.state == "ok", toast: ss(&e.toast),
            zoom: e.z, pan_x: e.px, pan_y: e.py, bx: 0.0, by: 0.0, bw: 0.0, bh: 0.0, conn: e.conn.is_some(), conn_path: ss(e.conn.clone().unwrap_or_default()),
        }
    });
    g(|f| {
        let mut x = ed;
        let old = f.get_ed();
        x.bx = old.bx; x.by = old.by; x.bw = old.bw; x.bh = old.bh;
        f.set_ed(x);
    });
}

fn known_skills() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    for (nm, d, _) in with(|e| e.skills.clone()) { if !out.iter().any(|x| x.0 == nm) { out.push((nm, d)); } }
    out
}
fn mentions(text: &str) -> Vec<PlanningFxCheck> {
    let known = known_skills();
    model::mentions(text).into_iter().map(|m| { let k = known.iter().find(|x| x.0 == m); PlanningFxCheck { id: ss(&m), label: ss(&m), sub: ss(k.map(|x| x.1.clone()).unwrap_or_default()), on: k.is_some(), enabled: true } }).collect()
}

fn render() {
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
        nodes.push(PlanningFxNode {
            id: ss(&id), kind: ss(&ty), kind_label: ss(model::kind_label(&ty).to_uppercase()), title: ss(model::node_title(node, &f)), body: ss(&body), lines,
            x: r.x, y: r.y, w, h, sel: sel.as_ref().map(|x| x.0 == "node" && x.1 == id).unwrap_or(false), bad: issues.iter().any(|i| i.0 == "error"), active: false, visited: false, retired: false,
            badge, badge_level: ss(level), token: "".into(), token_kind: "".into(), doing: false, doing_who: "".into(), doing_text: "".into(), doing_busy: false, skills: "".into(),
            outs: rc_of(outs.iter().map(|h2| PlanningFxOut { id: ss(h2), label: ss(model::handle_label(node, h2)), finish: h2 == "finish" }).collect()),
            agent: model::is_agent(&ty), has_in: ty != "start",
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
        edges.push(PlanningFxEdge {
            id: ss(&id), path: ss(path), arrow: ss(model::arrow(tx, ty, note)), tone: ss(tone), sel: sel.as_ref().map(|x| x.0 == "edge" && x.1 == id).unwrap_or(false), pulse: false,
            label: ss(model::edge_label(e, None)), icon: ss(if n(e, "maxRounds") > 0.0 && !note { "loop" } else if note { "note" } else { "" }), lx, ly,
        });
    }
    let bounds = model::union(&rects.values().copied().collect::<Vec<_>>()).unwrap_or(Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 });
    let people: Vec<PlanningFxOpt> = arr(&f, "roster").iter().filter(|r| s(r, "id") != "manager").map(|r| PlanningFxOpt { id: ss(s(r, "id")), label: ss({ let x = s(r, "name"); if x.is_empty() { "Unnamed".into() } else { x } }) }).collect();
    let fit_now = with(|e| { e.rects = rects; e.outs = outs_of; !e.fitted && e.cw > 0.0 });
    if fit_now { let (cw, ch) = with(|e| (e.cw, e.ch)); let (z, px, py) = model::fit(bounds, cw, ch, 0.15, 0.15, 1.0); with(|e| { e.z = z; e.px = px; e.py = py; e.fitted = true; }); }
    let (key, insp) = inspector(&f, &validation);
    let replace = with(|e| { let r = e.insp_key != key; if r { e.insp_key = key.clone(); e.insp_gen += 1; } r });
    super::models(|m| {
        kit::sync_rows(&m.e.nodes, nodes);
        kit::sync_rows(&m.e.edges, edges);
        kit::sync_rows(&m.e.people, people);
        if replace || m.e.insp.row_count() == 0 { m.e.insp.set_vec(vec![insp]); } else { m.e.insp.set_row_data(0, insp); }
    });
    push_ed();
    g(|f2| { let mut x = f2.get_ed(); x.bx = bounds.x; x.by = bounds.y; x.bw = bounds.w; x.bh = bounds.h; f2.set_ed(x); });
}

fn issue_rows(list: &[model::Issue]) -> Vec<PlanningFxIssue> {
    list.iter().map(|(lv, m, fix, raw)| {
        let key = { let e = s(raw, "employeeId"); if !e.is_empty() { format!("emp:{e}") } else { let nid = { let x = s(raw, "nodeId"); if !x.is_empty() { x } else { arr(raw, "nodeIds").first().and_then(|v| v.as_str()).unwrap_or("").to_string() } }; if !nid.is_empty() { format!("node:{nid}") } else { let ed = s(raw, "edgeId"); if ed.is_empty() { String::new() } else { format!("edge:{ed}") } } } };
        PlanningFxIssue { level: ss(lv), message: ss(m), fix: ss(fix), key: ss(key) }
    }).collect()
}

fn presets_list() -> Vec<(String, String)> { arr(&data(), "presets").iter().map(|p| (s(p, "key"), s(p, "label"))).collect() }
fn str_list(v: &Value) -> Vec<String> { v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default() }

/// The side's pane: (its key, its row). The key changes when the pane shows something else.
fn inspector(f: &Value, validation: &Value) -> (String, PlanningFxInsp) {
    let (tab, sel, roster_sel, secrets) = with(|e| (e.tab.clone(), e.sel.clone(), e.roster_sel.clone(), e.secrets.clone()));
    let (by_node, by_edge, by_emp) = model::issues_by(validation);
    let mut d = PlanningFxInsp::default();
    let roster = arr(f, "roster");
    let settings = &f["settings"];
    if tab == "team" {
        if let Some(p) = roster_sel.as_ref().and_then(|id| roster.iter().find(|r| s(r, "id") == *id)) {
            let id = s(p, "id");
            let mgr = id == "manager";
            d.mode = "person".into();
            d.manager = mgr;
            d.issues = rc_of(issue_rows(by_emp.get(&id).map(|v| v.as_slice()).unwrap_or(&[])));
            d.name = ss(s(p, "name")); d.persona = ss(s(p, "persona")); d.mentions_p = rc_of(mentions(&s(p, "persona")));
            d.model_i = ["", "opus", "sonnet", "haiku"].iter().position(|x| *x == s(p, "model")).unwrap_or(0) as i32;
            d.effort_i = ["", "low", "medium", "high", "xhigh", "max"].iter().position(|x| *x == s(p, "effort")).unwrap_or(0) as i32;
            let fresh = s(p, "memory") == "fresh";
            d.memory_i = fresh as i32;
            d.memory_hint = ss(if fresh { "A new session each time: useful for reviewers, who should look with fresh eyes." } else { "One session for the whole run: they remember earlier steps." });
            let deny = str_list(&p["tools"]["deny"]);
            d.can_edit = !EDIT_TOOLS.iter().any(|t| deny.iter().any(|x| x == t));
            d.pinned = ss(str_list(&p["skills"]).join(", "));
            let everyone = str_list(&settings["presets"]);
            let mine = str_list(&p["presets"]);
            d.show_presets = !mgr && s(settings, "teamMode") != "bypass";
            d.p_presets = rc_of(presets_list().into_iter().map(|(k, l)| { let all = everyone.contains(&k); PlanningFxCheck { id: ss(&k), label: ss(format!("{l}{}", if all { " (everyone has this)" } else { "" })), sub: "".into(), on: all || mine.contains(&k), enabled: !all } }).collect());
            d.allow = ss(str_list(&p["tools"]["allow"]).join(", ")); d.deny = ss(deny.join(", "));
            let used = model::steps_of(f, &id);
            d.steps_hint = ss(format!("In {used} step{} of the flow.", if used == 1 { "" } else { "s" }));
            return (format!("person:{id}"), d);
        }
        d.mode = "team".into();
        d.people = rc_of(roster.iter().map(|r| { let id = s(r, "id"); PlanningFxPerson { id: ss(&id), name: ss({ let x = s(r, "name"); if x.is_empty() { "Unnamed".into() } else { x } }), meta: ss(model::person_meta(f, &id, by_emp.get(&id).map(|l| l.len()).unwrap_or(0))), manager: id == "manager" } }).collect());
        let mut opts: Vec<PlanningFxOpt> = arr(&data(), "personas").iter().map(|p| PlanningFxOpt { id: ss(s(p, "key")), label: ss(s(p, "name")) }).collect();
        opts.push(PlanningFxOpt { id: "".into(), label: "Blank".into() });
        d.persona_rows = rc_of(opts.chunks(3).map(|c| PlanningFxOptRow { items: rc_of(c.to_vec()) }).collect());
        return ("team".into(), d);
    }
    let ok = b(validation, "ok");
    if tab == "issues" {
        let mut all = vec![];
        for (list, lv) in [("errors", "error"), ("warnings", "warning")] { for it in arr(validation, list) { all.push((lv.to_string(), s(it, "message"), s(it, "fix"), it.clone())); } }
        d.mode = "issues".into(); d.ok = ok;
        d.issues_title = ss(if ok { "Ready to run".to_string() } else { format!("{} to fix", arr(validation, "errors").len()) });
        d.issues_hint = ss(if ok && !all.is_empty() { "It can run. The warnings below are worth a look." } else if !ok { "It is saved as a draft and cannot run until these are fixed. Tap one to go to it." } else { "" });
        d.issues = rc_of(issue_rows(&all));
        return ("issues".into(), d);
    }
    if tab == "dry" { d.mode = "dry".into(); d.ok = ok; return ("dry".into(), d); }
    let names = |n2: &Value| -> String { let l = s(n2, "label"); if !l.is_empty() { return l; } if s(n2, "type") == "employee" { if let Some(r) = roster.iter().find(|r| s(r, "id") == s(n2, "employeeId")) { let x = s(r, "name"); if !x.is_empty() { return x; } } } model::kind_label(&s(n2, "type")).to_string() };
    if let Some((kind, id)) = sel.clone() {
        if kind == "node" {
            if let Some(node) = arr(f, "nodes").iter().find(|x| s(x, "id") == id) {
                let ty = s(node, "type");
                d.mode = "node".into(); d.kind = ss(&ty); d.kind_label = ss(model::kind_label(&ty)); d.can_delete = !["start", "manager", "end"].contains(&ty.as_str());
                d.issues = rc_of(issue_rows(by_node.get(&id).map(|v| v.as_slice()).unwrap_or(&[])));
                d.label = ss(s(node, "label")); d.label_ph = ss(model::kind_label(&ty));
                let emps: Vec<&Value> = roster.iter().filter(|r| s(r, "id") != "manager").collect();
                let mut who = vec!["Choose someone…".to_string()];
                who.extend(emps.iter().map(|r| { let x = s(r, "name"); if x.is_empty() { "Unnamed".into() } else { x } }));
                who.push("+ Add someone to the team…".into());
                d.who = strs(&who);
                let eid = s(node, "employeeId");
                d.who_i = emps.iter().position(|r| s(r, "id") == eid).map(|i| i + 1).unwrap_or(0) as i32;
                d.has_person = !eid.is_empty();
                let dc = &node["decision"];
                d.question = ss(s(dc, "question")); d.mentions_q = rc_of(mentions(&s(if ty == "ask" { &node["ask"] } else { dc }, if ty == "ask" { "prompt" } else { "question" })));
                d.dkind_i = match s(dc, "kind").as_str() { "one" => 1, "many" => 2, _ => 0 };
                d.options = rc_of(arr(dc, "options").iter().map(|o| PlanningFxOpt { id: ss(s(o, "id")), label: ss(s(o, "label")) }).collect());
                d.review = b(dc, "review");
                let mut ans = vec!["Whoever arrives".to_string()];
                ans.extend(roster.iter().map(|r| { let x = s(r, "name"); if x.is_empty() { s(r, "id") } else { x } }));
                d.answered = strs(&ans);
                let ab = s(dc, "answeredBy");
                d.answered_i = roster.iter().position(|r| s(r, "id") == ab && !ab.is_empty()).map(|i| i + 1).unwrap_or(0) as i32;
                d.prompt = ss(s(&node["ask"], "prompt"));
                d.replies = rc_of(arr(&node["ask"], "replies").iter().map(|o| PlanningFxOpt { id: ss(s(o, "id")), label: ss(s(o, "label")) }).collect());
                let w = &node["wait"];
                d.wkind_i = match s(w, "kind").as_str() { "signal" => 1, "time" => 2, "until" => 3, _ => 0 };
                let num = |k: &str| { let v = n(w, k) as i64; if w.get(k).map(|x| x.is_null()).unwrap_or(true) { String::new() } else { v.to_string() } };
                d.minutes = ss(num("minutes")); d.at = ss(s(w, "at")); d.command = ss(s(w, "command")); d.every = ss(num("every")); d.what = ss(s(w, "what")); d.timeout = ss(num("timeout"));
                let forks: Vec<&Value> = arr(f, "nodes").iter().filter(|x| s(x, "type") == "decision" && s(&x["decision"], "kind") == "many").collect();
                let mut fl = vec!["Choose a decision…".to_string()];
                fl.extend(forks.iter().map(|x| { let l = s(x, "label"); if !l.is_empty() { l } else { let q = s(&x["decision"], "question"); if q.is_empty() { "Decision".into() } else { q } } }));
                d.forks = strs(&fl);
                let fid = s(&node["join"], "forkId");
                d.fork_i = forks.iter().position(|x| s(x, "id") == fid && !fid.is_empty()).map(|i| i + 1).unwrap_or(0) as i32;
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
            d.edge_desc = ss(format!("{}{} → {}", nm(src), if !note && src.is_some() && sh != "out" { format!(" ({})", model::handle_label(src.unwrap(), &sh)) } else { String::new() }, nm(dst)));
            d.label = ss(format!("A note carries a message, not control: it waits in {}'s inbox until the flow reaches them.", nm(dst)));
            d.issues = rc_of(issue_rows(by_edge.get(&id).map(|v| v.as_slice()).unwrap_or(&[])));
            d.note_label = ss(s(e, "noteLabel"));
            let mr = n(e, "maxRounds") as i64;
            d.rounds = ss(if mr > 0 { mr.to_string() } else { String::new() });
            d.require_note = b(e, "requireNote");
            return (format!("edge:{id}"), d);
        }
    }
    // The factory's settings.
    d.mode = "settings".into();
    d.name = ss(s(f, "name")); d.description = ss(s(f, "description")); d.mentions_d = rc_of(mentions(&s(f, "description")));
    let crt = arr(settings, "crTemplate");
    d.cr = rc_of(crt.iter().map(|x| PlanningFxCrSec { id: ss(s(x, "id")), label: ss(s(x, "label")), hint: ss(s(x, "hint")), required: b(x, "required"), list: b(x, "list") }).collect());
    let pick = |v: &[Value]| v.iter().map(|x| json!({ "id": s(x, "id"), "label": s(x, "label"), "hint": s(x, "hint"), "required": b(x, "required"), "list": b(x, "list") })).collect::<Vec<_>>();
    let dd = data();
    d.cr_default = pick(crt) == pick(arr(&dd, "crTemplate"));
    d.cr_full = crt.len() as f64 >= n(&dd["crLimits"], "sections").max(1.0);
    let bypass = s(settings, "managerMode") == "bypass";
    let tbypass = s(settings, "teamMode") == "bypass";
    let gk = b(&settings["gatekeeper"], "enabled");
    d.mmode_i = bypass as i32;
    d.mmode_hint = ss(if bypass { "The Manager runs unchecked and approves its team's requests itself. Choose this only if you trust it. Some things stay refused: sudo, restarting Crundi, pushing, the server's ports." } else { "The Manager's own actions ask you. It still decides its team's permission requests." });
    d.tmode_i = tbypass as i32;
    d.gatekeeper = gk;
    d.tmode_hint = ss(if tbypass { "The team runs unchecked too: nobody asks for anything, which is fastest. The fixed refusals still apply, but nothing stops them writing outside their worktree, so only for projects you trust them with." } else if gk { "Requests go to the Gatekeeper, which passes the unclear ones to the Manager." } else { "Requests go to the Manager, whose decision is final." });
    d.bypass_warn = bypass && !tbypass;
    let everyone = str_list(&settings["presets"]);
    d.presets = rc_of(presets_list().into_iter().map(|(k, l)| PlanningFxCheck { on: everyone.contains(&k), id: ss(k), label: ss(l), sub: "".into(), enabled: true }).collect());
    d.rules = ss(str_list(&settings["rules"]).join("\n"));
    d.manager_rules = b(settings, "managerRules");
    d.mrules_hint = ss(if d.manager_rules { "So it is not asked the same thing again and again: a rule it grants applies to that person (or everyone, if it says so) for the rest of the run." } else { "It answers every request on its own." });
    d.gk_i = ["haiku", "sonnet", "opus"].iter().position(|x| *x == s(&settings["gatekeeper"], "model")).unwrap_or(0) as i32;
    if let Some(list) = secrets {
        let chosen = str_list(&settings["secrets"]);
        d.secrets_on = true;
        d.secrets = rc_of(list.iter().map(|x| PlanningFxCheck { id: ss(s(x, "id")), label: ss(s(x, "name")), sub: ss(s(x, "description")), on: chosen.contains(&s(x, "id")), enabled: true }).collect());
        d.secrets_gone = strs(&chosen.iter().filter(|id| !list.iter().any(|x| s(x, "id") == **id)).cloned().collect::<Vec<_>>());
    }
    d.merge_i = (s(settings, "mergeMode") == "auto") as i32;
    d.auto_recover = b(settings, "autoRecover");
    d.setup = ss(s(settings, "setupCommand"));
    d.max_handoffs = ss((n(&settings["caps"], "maxHandoffs") as i64).to_string());
    d.max_minutes = ss((n(&settings["caps"], "maxMinutes") as i64).to_string());
    let tick = n(settings, "tickMinutes") as i64;
    d.tick = ss(if tick > 0 { tick.to_string() } else { String::new() });
    d.tick_hint = ss(if tick > 0 { format!("A wake-up every {tick} minutes, only while the Manager is idle; skipped otherwise. Each one is a short Manager turn.") } else { "Blank: the Manager chooses at the start of each run (5 minutes to 24 hours). A wake-up only while it is idle, so it can check on things.".into() });
    let lim = &settings["limits"];
    d.l5h = ss((n(lim, "pauseAt5h") as i64).to_string()); d.lweek = ss((n(lim, "pauseAtWeek") as i64).to_string());
    d.lpar = ss((n(lim, "maxParallel") as i64).to_string()); d.lone = ss((n(lim, "oneAtATimeAbove") as i64).to_string());
    d.lim_hint = ss(if n(lim, "pauseAt5h") < 100.0 || n(lim, "pauseAtWeek") < 100.0 { "It pauses there and carries on by itself after the window resets, leaving the rest for you." } else { "At 100% it pauses only when a window is full, and carries on by itself after the reset." });
    ("settings".into(), d)
}

// ─── The canvas ───

pub fn size(w: f32, h: f32) {
    let first = with(|e| { let f = !e.fitted; e.cw = w; e.ch = h; f && e.state == "ok" });
    if first { render(); }
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
    render();
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
    if ok { render(); }
}
pub fn node_up(_id: &str, moved: bool) {
    let changed = with(|e| {
        let Some((_, _, _, snap, did)) = e.drag.take() else { return false };
        if !(moved && did) || snap == e.f { return false; }
        e.past.push(snap); if e.past.len() > 100 { e.past.remove(0); } e.future.clear(); e.dirty = true; e.save = "dirty".into(); e.last_key.clear();
        true
    });
    if changed { schedule_save(); render(); }
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

pub fn handle_up(node: &str, h: &str, x: f32, y: f32) {
    let (fx, fy) = flow_of(x, y);
    let target = with(|e| {
        e.conn = None;
        let pad = 30.0 / e.z;
        e.rects.iter().filter(|(id, _)| id.as_str() != node).find(|(_, r)| fx >= r.x - pad && fx <= r.x + r.w + pad && fy >= r.y - pad && fy <= r.y + r.h + pad).map(|(id, _)| id.clone())
    });
    push_ed();
    if let Some(t) = target { connect_ids(node, h, &t); }
}

/// onConnect(): a valid connection is added; one that closes a loop gets a limit.
pub fn connect_ids(src: &str, h: &str, tgt: &str) -> bool {
    let th = if h == "note" { "notein" } else { "in" };
    let ok = with(|e| model::can_connect(&e.f, src, h, tgt, th));
    if !ok { return false; }
    let looped = Rc::new(RefCell::new(false));
    let l2 = looped.clone();
    let (src, h, tgt) = (src.to_string(), h.to_string(), tgt.to_string());
    mutate("", move |f| { *l2.borrow_mut() = model::connect(f, &src, &h, &tgt); });
    if *looped.borrow() { flash("That closes a loop, so it got a round limit of 3. Change it on the connection."); }
    true
}

pub fn edge_click(id: &str) { select("edge", id); }
pub fn pane_click() { if with(|e| e.sel.is_some()) { with(|e| e.sel = None); render(); } }

pub fn key(k: &str) {
    if k != "delete" { return; }
    let sel = with(|e| e.sel.clone());
    match sel {
        Some((kind, id)) if kind == "node" => {
            let deletable = with(|e| arr(&e.f, "nodes").iter().any(|x| s(x, "id") == id && !["start", "manager", "end"].contains(&s(x, "type").as_str())));
            if deletable { mutate("", |f| model::remove_nodes(f, &[id])); with(|e| e.sel = None); render(); }
        }
        Some((_, id)) => { mutate("", |f| model::remove_edge(f, &id)); with(|e| e.sel = None); render(); }
        None => {}
    }
}

/// addAt(): a new step at a point of the canvas (the middle of the view when none).
fn add_at(kind: &str, at: Option<(f32, f32)>) {
    let (fx, fy) = match at { Some((x, y)) => flow_of(x, y), None => { let (cw, ch) = with(|e| (e.cw, e.ch)); flow_of(cw / 2.0, ch / 2.0) } };
    let (sx, sy) = (((fx / 16.0).round() * 16.0) as f64, ((fy / 16.0).round() * 16.0) as f64);
    let node = model::new_node(kind, sx, sy);
    let id = s(&node, "id");
    let k2 = kind.to_string();
    mutate("", move |f| {
        let mut nodes = arr(f, "nodes").to_vec();
        nodes.push(node.clone());
        if k2 == "decision:many" { let mut j = model::new_node("join", sx, sy + 240.0); j["label"] = json!("Join"); j["join"] = json!({ "forkId": s(&node, "id") }); nodes.push(j); }
        f["nodes"] = Value::Array(nodes);
    });
    with(|e| {
        e.sel = Some(("node".into(), id.clone()));
        if kind == "employee:" { e.roster_add_to = Some(id.clone()); e.roster_sel = None; e.tab = "team".into(); } else { e.tab = "edit".into(); }
    });
    render();
}

fn tidy() {
    let heights: HashMap<String, f32> = with(|e| e.rects.iter().map(|(k, r)| (k.clone(), r.h)).collect());
    let pos = with(|e| model::tidy(&e.f, &heights));
    mutate("", move |f| {
        if let Some(nodes) = f["nodes"].as_array_mut() { for nd in nodes.iter_mut() { if let Some((x, y)) = pos.get(&s(nd, "id")) { nd["position"] = json!({ "x": x, "y": y }); } } }
    });
    control("fit");
}

fn pick_issue(key: &str) {
    if let Some(id) = key.strip_prefix("emp:") { with(|e| { e.roster_sel = Some(id.to_string()); e.tab = "team".into(); }); render(); return; }
    if let Some(id) = key.strip_prefix("node:") {
        select("node", id);
        with(|e| if let Some(r) = e.rects.get(id).copied() { let z = e.z.max(0.9); e.z = z; e.px = e.cw / 2.0 - (r.x + r.w / 2.0) * z; e.py = e.ch / 2.0 - (r.y + 40.0) * z; });
        push_ed();
        return;
    }
    if let Some(id) = key.strip_prefix("edge:") { select("edge", id); }
}

fn remove_person() {
    let Some(id) = with(|e| e.roster_sel.clone()) else { return };
    let (name, steps) = with(|e| (arr(&e.f, "roster").iter().find(|r| s(r, "id") == id).map(|r| s(r, "name")).filter(|x| !x.is_empty()).unwrap_or_else(|| "this person".into()), model::steps_of(&e.f, &id)));
    let msg = format!("Remove {name} from the team{}?", if steps > 0 { format!(" and their {steps} step{} from the flow", if steps == 1 { "" } else { "s" }) } else { String::new() });
    kit::confirm(kit::Confirm { title: "Remove", message: &msg, ok: "Remove", danger: true }, move |ok| {
        if !ok { return; }
        let id2 = id.clone();
        mutate("", move |f| {
            let gone: Vec<String> = arr(f, "nodes").iter().filter(|x| s(x, "type") == "employee" && s(x, "employeeId") == id2).map(|x| s(x, "id")).collect();
            let set: HashSet<String> = gone.iter().cloned().collect();
            let nodes: Vec<Value> = arr(f, "nodes").iter().filter(|x| !set.contains(&s(x, "id"))).cloned().map(|mut x| { if s(&x, "type") == "decision" && s(&x["decision"], "answeredBy") == id2 { x["decision"]["answeredBy"] = Value::Null; } x }).collect();
            let edges: Vec<Value> = arr(f, "edges").iter().filter(|e| !set.contains(&s(e, "source")) && !set.contains(&s(e, "target"))).cloned().collect();
            let roster: Vec<Value> = arr(f, "roster").iter().filter(|r| s(r, "id") != id2).cloned().collect();
            f["nodes"] = Value::Array(nodes); f["edges"] = Value::Array(edges); f["roster"] = Value::Array(roster);
        });
        with(|e| e.roster_sel = None);
        render();
    });
}

fn add_person(key: &str) {
    let dd = data();
    let tpl = arr(&dd, "personas").iter().find(|p| s(p, "key") == key).cloned();
    let skills: Vec<String> = known_skills().into_iter().map(|x| x.0).collect();
    let emp = with(|e| model::new_employee(tpl.as_ref(), &e.f, if skills.is_empty() { None } else { Some(skills.as_slice()) }));
    let id = s(&emp, "id");
    let add_to = with(|e| e.roster_add_to.clone());
    let id2 = id.clone();
    mutate("", move |f| {
        let mut r = arr(f, "roster").to_vec(); r.push(emp); f["roster"] = Value::Array(r);
        // The step it was added for now names them.
        if let Some(nid) = add_to { if let Some(nd) = node_mut(f, &nid) { nd["employeeId"] = json!(id2); } }
    });
    with(|e| e.roster_sel = Some(id));
    render();
}

pub fn act(a: &str, x: &str, _y: &str) {
    match a {
        "e-back" => { save_now(|_| super::close_sub()); }
        "e-save" => { with(|e| e.dirty = true); save_now(|_| {}); }
        "e-reload" => reload(),
        "e-undo" => undo(),
        "e-redo" => redo(),
        "e-tidy" => tidy(),
        "e-settings" => { with(|e| { e.sel = None; e.tab = "edit".into(); }); render(); }
        "e-tab" => { with(|e| e.tab = x.to_string()); render(); }
        "e-add" => add_at(x, None),
        "e-add-at" => {
            let parts: Vec<f32> = _y.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(-1.0)).collect();
            let (px, py) = (parts.first().copied().unwrap_or(-1.0), parts.get(1).copied().unwrap_or(-1.0));
            let (cw, ch) = with(|e| (e.cw, e.ch));
            if px >= 0.0 && py >= 0.0 && px <= cw && py <= ch { add_at(x, Some((px, py))); }
        }
        "e-delete" => key("delete"),
        "e-person" => {
            let id = if x.is_empty() { with(|e| e.sel.clone().and_then(|(k, id)| if k == "node" { arr(&e.f, "nodes").iter().find(|n2| s(n2, "id") == id).map(|n2| s(n2, "employeeId")) } else { None })).unwrap_or_default() } else { x.to_string() };
            with(|e| { e.roster_sel = Some(id); e.tab = "team".into(); });
            render();
        }
        "e-team" => { with(|e| { e.roster_sel = None; e.roster_add_to = None; }); render(); }
        "e-add-person" => add_person(x),
        "e-remove-person" => remove_person(),
        "e-issue" => pick_issue(x),
        _ => {}
    }
}

// ─── The fields ───

fn clamp_num(v: &str, lo: i64, hi: i64) -> i64 { let t = v.trim(); if t.is_empty() { 0 } else { t.parse::<f64>().map(|x| (x.round() as i64).clamp(lo, hi)).unwrap_or(lo) } }
fn list_of(v: &str, sep: char) -> Value { json!(v.split(sep).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect::<Vec<_>>()) }
fn set_in(f: &mut Value, path: &[&str], v: Value) { let mut cur = f; for p in &path[..path.len() - 1] { if !cur[*p].is_object() { cur[*p] = json!({}); } cur = &mut cur[*p]; } cur[path[path.len() - 1]] = v; }

/// A field of the pane changed (Inspector.jsx's set()).
pub fn edit(k: &str, v: &str) {
    let (sel, roster_sel, tab) = with(|e| (e.sel.clone(), e.roster_sel.clone(), e.tab.clone()));
    let v2 = v.to_string();
    // A person (the Team pane).
    if tab == "team" {
        let Some(pid) = roster_sel else { return };
        let key = format!("{k}:{pid}");
        let pid2 = pid.clone();
        let k2 = k.to_string();
        mutate(if ["p-name", "persona", "pinned", "allow", "deny"].contains(&k) { key.as_str() } else { "" }, move |f| {
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
            // The roster's new person from "Who does this step".
            if k == "who" {
                let i: usize = v.parse().unwrap_or(0);
                let emps: Vec<String> = with(|e| arr(&e.f, "roster").iter().filter(|r| s(r, "id") != "manager").map(|r| s(r, "id")).collect());
                if i == emps.len() + 1 { with(|e| { e.roster_add_to = Some(id.clone()); e.roster_sel = None; e.tab = "team".into(); }); render(); return; }
                let eid = if i == 0 { String::new() } else { emps.get(i - 1).cloned().unwrap_or_default() };
                mutate("", move |f| { if let Some(nd) = node_mut(f, &id2) { nd["employeeId"] = json!(eid); } });
                return;
            }
            let roster_ids: Vec<String> = with(|e| arr(&e.f, "roster").iter().map(|r| s(r, "id")).collect());
            let forks: Vec<String> = with(|e| arr(&e.f, "nodes").iter().filter(|x| s(x, "type") == "decision" && s(&x["decision"], "kind") == "many").map(|x| s(x, "id")).collect());
            let text = ["label", "question", "prompt", "minutes", "wat", "command", "every", "what", "timeout"].contains(&k) || k.ends_with("-label") || k.contains("-label:");
            let key = format!("{k}:{id}");
            let k2 = k.to_string();
            mutate(if text { key.as_str() } else { "" }, move |f| {
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
                        // Options and replies: label:<id>, remove, add.
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
            // A new option or reply: the pane shows the list again.
            if k.ends_with("-add") || k.ends_with("-remove") || k == "dkind" || k == "wkind" { with(|e| e.insp_key.clear()); render(); }
        }
        Some((_, id)) => {
            let id2 = id.clone();
            let key = format!("{k}:{id}");
            let k2 = k.to_string();
            mutate(if k == "require-note" { "" } else { key.as_str() }, move |f| {
                let Some(e) = f["edges"].as_array_mut().and_then(|a| a.iter_mut().find(|e| s(e, "id") == id2)) else { return };
                match k2.as_str() { "note-label" => e["noteLabel"] = json!(v2), "rounds" => e["maxRounds"] = json!(clamp_num(&v2, 0, 100)), "require-note" => e["requireNote"] = json!(v2 == "1"), _ => {} }
            });
            if k == "require-note" { with(|e| e.insp_key.clear()); render(); }
        }
        None => edit_settings(k, v),
    }
}

fn node_mut<'a>(f: &'a mut Value, id: &str) -> Option<&'a mut Value> { f["nodes"].as_array_mut().and_then(|a| a.iter_mut().find(|x| s(x, "id") == id)) }

fn edit_settings(k: &str, v: &str) {
    let v2 = v.to_string();
    let k2 = k.to_string();
    let texty = ["name", "description", "rules", "setup", "max-handoffs", "max-minutes", "tick", "l5h", "lweek", "lpar", "lone"].contains(&k) || k.starts_with("cr-label:") || k.starts_with("cr-hint:");
    let restructure = k.starts_with("cr-up") || k.starts_with("cr-down") || k.starts_with("cr-remove") || k == "cr-add" || k == "cr-default" || k == "tmode" || k == "mmode" || k == "gatekeeper" || k.starts_with("secret:");
    mutate(if texty { k } else { "" }, move |f| {
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
    if restructure { with(|e| e.insp_key.clear()); render(); }
}

pub fn props(f: &PlanningFactories) -> Value {
    let ed = f.get_ed();
    let insp = f.get_e_insp().row_data(0);
    json!({
        "state": ed.state.as_str(), "error": ed.error.as_str(), "name": ed.name.as_str(), "save": ed.save.as_str(), "saveLabel": ed.save_label.as_str(), "draft": ed.draft,
        "canUndo": ed.can_undo, "canRedo": ed.can_redo, "tab": ed.tab.as_str(), "issuesLabel": ed.issues_label.as_str(), "toast": ed.toast.as_str(), "zoom": ed.zoom, "conn": ed.conn,
        "id": with(|e| e.id.clone()), "sel": with(|e| e.sel.clone().map(|x| format!("{}:{}", x.0, x.1)).unwrap_or_default()),
        "nodes": f.get_e_nodes().iter().map(|n2| json!({ "id": n2.id.as_str(), "kind": n2.kind.as_str(), "title": n2.title.as_str(), "body": n2.body.as_str(), "x": n2.x, "y": n2.y, "sel": n2.sel, "badge": n2.badge, "outs": n2.outs.iter().map(|o| json!({ "id": o.id.as_str(), "label": o.label.as_str() })).collect::<Vec<_>>() })).collect::<Vec<_>>(),
        "edges": f.get_e_edges().iter().map(|e| json!({ "id": e.id.as_str(), "tone": e.tone.as_str(), "label": e.label.as_str(), "sel": e.sel })).collect::<Vec<_>>(),
        "people": f.get_e_people().iter().map(|p| p.label.to_string()).collect::<Vec<_>>(),
        "pane": insp.map(|d| json!({ "mode": d.mode.as_str(), "kindLabel": d.kind_label.as_str(), "label": d.label.as_str(), "name": d.name.as_str(), "issues": d.issues.iter().map(|i| i.message.to_string()).collect::<Vec<_>>(),
            "people": d.people.iter().map(|p| p.name.to_string()).collect::<Vec<_>>(), "dkind": d.dkind_i, "options": d.options.iter().map(|o| o.label.to_string()).collect::<Vec<_>>(), "rounds": d.rounds.as_str(),
            "issuesTitle": d.issues_title.as_str(), "persona": d.persona.as_str(), "edgeDesc": d.edge_desc.as_str() })).unwrap_or(Value::Null),
    })
}
