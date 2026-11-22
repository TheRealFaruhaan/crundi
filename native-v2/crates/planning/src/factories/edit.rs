//! Factories (4.7): the factory editor (tools/factory-flow/src/Editor.jsx,
//! Palette, Inspector, Roster, CrTemplate). The factory stays the source of
//! truth: every change edits its JSON (undo and redo keep copies, typing
//! in one field coalesces), the canvas is drawn from it, and it is saved
//! 1.2 s after the last change (Ctrl+S at once; Saved / Unsaved / Saving… /
//! Changed elsewhere with Reload / Not saved with Retry).
//!
//! As the web, the factory is checked here as it changes (valid.rs, the
//! server's validateFactory), so its issues show at once; the Dry run walks
//! the flow with the server's engine (engine.rs) without Claude; typing "/"
//! in a question, a prompt, the description or a persona offers the skills.
//! Narrow (the web's phone): no palette; Add, Edit, Team, Issues and Dry run
//! are a bar at the bottom, each opening its pane as a sheet over the canvas.

use super::model::{self, arr, b, n, s, Rect};
use super::parts::{self, btn, field, note, sub, Check, CrSec, Edge, Issue, Node, Opt, Out, Person};
use super::{act as fact, edit_field, Fx};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::{HashMap, HashSet}, rc::Rc};

/// The editor's top bar (PlanningFxEd).
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Ed {
    pub state: String, pub error: String, pub name: String, pub save: String, pub save_label: String, pub draft: bool,
    pub can_undo: bool, pub can_redo: bool, pub tab: String, pub issues_label: String, pub issues_bad: bool,
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
pub(crate) struct E {
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
    save_gen: u64,
    /// The narrow editor's first draw closed its sheet (the web starts a phone with no tab).
    phone_seen: bool,
    insp_key: String,
    state: String,
    error: String,
    rects: HashMap<String, Rect>,
    outs: HashMap<String, Vec<String>>,
    gn: u64,
    cr_open: bool,
    more_open: bool,
    /// Narrow (the web's phone), as last drawn.
    phone: bool,
    /// The Dry run (DryRun.jsx): the run being walked, what is answered for
    /// whoever has the work, the notes, the last refusal, the log.
    run: Option<Value>,
    dry_answers: serde_json::Map<String, Value>,
    dry_notes: serde_json::Map<String, Value>,
    dry_err: String,
    dry_log: Vec<String>,
    dry_same: bool,
    dry_tree: u64,
    // What it shows.
    ed: Ed,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    people: Vec<Opt>,
    insp: Option<Insp>,
}

pub(crate) fn blank() -> E { E { z: 1.0, ..Default::default() } }
fn with<R>(cx: &mut App, f: impl FnOnce(&mut E) -> R) -> R { super::with_fx(cx, |x| f(&mut x.e)) }
/// While drawing: the editor's state and the tab's, from the store being drawn.
fn re<R>(fx: &mut Fx, f: impl FnOnce(&mut E) -> R) -> R { f(&mut fx.e) }
#[allow(dead_code)]
fn rf<R>(fx: &mut Fx, f: impl FnOnce(&mut super::F) -> R) -> R { f(&mut fx.f) }

fn data() -> Value { serde_json::from_str(super::data::DATA).unwrap_or(Value::Null) }
const EDIT_TOOLS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// Closing the editor: what is unsaved is saved first.
pub fn reset(cx: &mut App) {
    let (open, dirty) = with(cx, |e| (e.open, e.dirty));
    if open && dirty { save_now(cx, |_, _| {}); }
    reset_quiet(cx);
}
pub fn reset_quiet(cx: &mut App) { with(cx, |e| { let (cw, ch) = (e.cw, e.ch); *e = E { z: 1.0, cw, ch, ..Default::default() }; }); }
pub fn zoom(cx: &mut App) -> f32 { with(cx, |e| e.z) }

/// mountEditor(): load the factory, then the skills and the secrets.
pub fn open(id: &str, cx: &mut App) {
    let gn = with(cx, |e| { let (cw, ch, g2) = (e.cw, e.ch, e.gn + 1); *e = E { open: true, id: id.to_string(), z: 1.0, cw, ch, tab: "edit".into(), save: "saved".into(), state: "loading".into(), gn: g2, ..Default::default() }; g2 });
    parts::pwith(cx, |p| parts::forget(p, "ed|"));
    push_ed(cx);
    parts::get(cx, format!("/api/factories/get?id={}", parts::enc(id)), move |j, cx| {
        if with(cx, |e| e.gn != gn) { return; }
        if !b(&j, "ok") {
            let er = parts::err_text(&j, "Could not open this factory.");
            with(cx, |e| { e.state = "error".into(); e.error = er; });
            push_ed(cx);
            return;
        }
        with(cx, |e| { e.f = j["factory"].clone(); e.version = n(&j["factory"], "version") as i64; e.validation = j["validation"].clone(); e.state = "ok".into(); });
        render_model(cx);
    });
    parts::get(cx, "/api/skills".into(), move |j, cx| {
        if with(cx, |e| e.gn != gn) { return; }
        let list: Vec<(String, String, String)> = arr(&j, "skills").iter().map(|x| (s(x, "name"), s(x, "description"), s(x, "scope"))).collect();
        with(cx, |e| e.skills = list);
        render_model(cx);
    });
    super::caps(cx, move |c, cx| {
        if !c.iter().any(|x| x == "factory-secrets") { return; }
        parts::get(cx, "/api/secrets".into(), move |j, cx| {
            if with(cx, |e| e.gn != gn) || !b(&j, "ok") { return; }
            with(cx, |e| e.secrets = Some(arr(&j, "secrets").to_vec()));
            render_model(cx);
        });
    });
}

// ─── Changes, undo, redo ───

fn mutate(key: &str, cx: &mut App, f: impl FnOnce(&mut Value)) {
    let ok = with(cx, |e| {
        if !e.open || e.state != "ok" || e.run.is_some() { return false; }
        let now = model::now_ms();
        let coalesce = !key.is_empty() && e.last_key == key && now - e.last_at < 1500.0;
        e.last_key = key.to_string(); e.last_at = now;
        if !coalesce { e.past.push(e.f.clone()); if e.past.len() > 100 { e.past.remove(0); } e.future.clear(); }
        f(&mut e.f);
        e.dirty = true; e.save = "dirty".into();
        true
    });
    if ok { schedule_save(cx); render_model(cx); }
}

fn undo(cx: &mut App) {
    let ok = with(cx, |e| { if e.run.is_some() { return false; } let Some(p) = e.past.pop() else { return false }; let cur = std::mem::replace(&mut e.f, p); e.future.push(cur); e.last_key.clear(); e.dirty = true; e.save = "dirty".into(); true });
    if ok { schedule_save(cx); render_model(cx); }
}
fn redo(cx: &mut App) {
    let ok = with(cx, |e| { if e.run.is_some() { return false; } let Some(p) = e.future.pop() else { return false }; let cur = std::mem::replace(&mut e.f, p); e.past.push(cur); e.last_key.clear(); e.dirty = true; e.save = "dirty".into(); true });
    if ok { schedule_save(cx); render_model(cx); }
}

// ─── Saving ───

fn schedule_save(cx: &mut App) {
    let gn = with(cx, |e| { e.save_gen += 1; e.save_gen });
    parts::after(cx, 1200, move |cx| { if with(cx, |e| e.save_gen == gn) { save_now(cx, |_, _| {}); } });
}

/// doSave(): true when saved (or nothing to save).
fn save_now(cx: &mut App, then: impl FnOnce(bool, &mut App) + 'static) {
    let job = with(cx, |e| {
        e.save_gen += 1;
        if !e.dirty || !e.open { return None; }
        e.dirty = false; e.save = "saving".into();
        Some((e.id.clone(), e.f.clone(), e.version, e.gn))
    });
    let Some((id, f, version, gn)) = job else { then(true, cx); return };
    push_ed(cx);
    parts::post(cx, "/api/factories", json!({ "action": "save", "id": id, "definition": f, "baseVersion": version }), move |r, cx| {
        let saved = if with(cx, |e| e.gn != gn) { false } else if b(&r, "ok") {
            with(cx, |e| { e.version = n(&r["factory"], "version") as i64; if r.get("validation").is_some() { e.validation = r["validation"].clone(); } e.save = if e.dirty { "dirty".into() } else { "saved".into() }; });
            true
        } else if b(&r, "conflict") { with(cx, |e| e.save = "conflict".into()); false } else { with(cx, |e| { e.dirty = true; e.save = "error".into(); }); false };
        render_model(cx);
        then(saved, cx);
    });
}

fn reload(cx: &mut App) {
    let (id, gn) = with(cx, |e| (e.id.clone(), e.gn));
    parts::get(cx, format!("/api/factories/get?id={}", parts::enc(&id)), move |j, cx| {
        if with(cx, |e| e.gn != gn) || !b(&j, "ok") { return; }
        with(cx, |e| { e.f = j["factory"].clone(); e.version = n(&j["factory"], "version") as i64; e.validation = j["validation"].clone(); e.past.clear(); e.future.clear(); e.dirty = false; e.save = "saved".into(); });
        render_model(cx);
    });
}

/// flash(): a word about what just happened (the shared toast).
fn flash(t: &str, cx: &mut App) { parts::toast(cx, t); }

// ─── Building what it shows ───

fn push_ed(cx: &mut App) {
    with(cx, |e| {
        let ok = b(&e.validation, "ok");
        let errs = arr(&e.validation, "errors").len();
        let old = e.ed.clone();
        e.ed = Ed {
            state: e.state.clone(), error: e.error.clone(), name: s(&e.f, "name"), save: e.save.clone(),
            save_label: match e.save.as_str() { "saving" => "Saving…", "dirty" => "Unsaved", "conflict" => "Changed elsewhere", "error" => "Not saved", _ => "Saved" }.into(),
            draft: e.state == "ok" && !ok, can_undo: !e.past.is_empty(), can_redo: !e.future.is_empty(), tab: e.tab.clone(),
            issues_label: if ok { "Issues".to_string() } else { format!("Fix {errs}") }, issues_bad: !ok && e.state == "ok",
            zoom: e.z, pan_x: e.px, pan_y: e.py, bx: old.bx, by: old.by, bw: old.bw, bh: old.bh, conn: e.conn.is_some(), conn_path: e.conn.clone().unwrap_or_default(),
        };
    });
}

/// uniqueSkills: each skill once, by name (name, description).
fn known_skills(cx: &mut App) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    for (nm, d, _) in with(cx, |e| e.skills.clone()) { if !out.iter().any(|x| x.0 == nm) { out.push((nm, d)); } }
    out
}
fn mentions(cx: &mut App, text: &str) -> Vec<Check> {
    let known = known_skills(cx);
    model::mentions(text).into_iter().map(|m| { let k = known.iter().find(|x| x.0 == m); Check { id: m.clone(), label: m, sub: k.map(|x| x.1.clone()).unwrap_or_default(), on: k.is_some(), enabled: true } }).collect()
}

fn render_model(cx: &mut App) {
    let st = with(cx, |e| e.state.clone());
    if st != "ok" { push_ed(cx); return; }
    let (f, sel, skills) = with(cx, |e| (e.f.clone(), e.sel.clone(), e.skills.clone()));
    // validateFactory(f, { skills: the global ones, once they are known }), here, at each change.
    let global: HashSet<String> = skills.iter().filter(|x| x.2 == "global" || x.2 == "synced").map(|x| x.0.clone()).collect();
    let validation = super::valid::validate(&f, if skills.is_empty() { None } else { Some(&global) });
    with(cx, |e| e.validation = validation.clone());
    let (by_node, by_edge, _) = model::issues_by(&validation);
    // A dry run on show: where its work is, and how many rounds each loop used.
    let run = with(cx, |e| e.run.clone());
    let mut tokens_at: HashMap<String, Vec<Value>> = HashMap::new();
    if let Some(r) = &run { for t in arr(r, "tokens") { tokens_at.entry(s(t, "nodeId")).or_default().push(t.clone()); } }
    let mut nodes = vec![];
    let mut rects: HashMap<String, Rect> = HashMap::new();
    let mut outs_of: HashMap<String, Vec<String>> = HashMap::new();
    for node in arr(&f, "nodes") {
        let id = s(node, "id");
        let ty = { let t = s(node, "type"); if ["start", "manager", "employee", "decision", "join", "ask", "wait", "end"].contains(&t.as_str()) { t } else { "employee".into() } };
        let issues = by_node.get(&id).cloned().unwrap_or_default();
        let (badge, level) = model::badge_of(&issues);
        // The badge's title: the errors (else the warnings), each with its fix.
        let errs: Vec<&model::Issue> = issues.iter().filter(|i| i.0 == "error").collect();
        let shown: Vec<&model::Issue> = if errs.is_empty() { issues.iter().collect() } else { errs };
        let badge_tip = shown.iter().map(|i| format!("{}{}", i.1, if i.2.is_empty() { String::new() } else { format!("\n  {}", i.2) })).collect::<Vec<_>>().join("\n");
        let body = model::node_body(node, &f);
        let outs = model::outputs_of(node);
        let w = model::node_w(&ty);
        let lines = model::body_lines(&body, w);
        let h = model::node_h(lines, false, outs.len());
        let r = Rect { x: n(&node["position"], "x") as f32, y: n(&node["position"], "y") as f32, w, h };
        rects.insert(id.clone(), r);
        let toks = tokens_at.get(&id).cloned().unwrap_or_default();
        let (token, token_kind) = model::token_pill(&toks, false);
        nodes.push(Node {
            token, token_kind, active: !toks.is_empty(), badge_tip,
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
        let used = run.as_ref().map(|r| n(&r["edgeCounts"], &id) as i64);
        let tone = if bad { "bad" } else if used.unwrap_or(0) > 0 { "used" } else if n(e, "maxRounds") > 0.0 && !note { "loop" } else if note { "note" } else { "flow" };
        edges.push(Edge {
            id: id.clone(), path, arrow: model::arrow(tx, ty, note), tone: tone.into(), sel: sel.as_ref().map(|x| x.0 == "edge" && x.1 == id).unwrap_or(false), pulse: false,
            label: model::edge_label(e, used), icon: if n(e, "maxRounds") > 0.0 && !note { "loop".into() } else if note { "note".into() } else { String::new() }, lx, ly,
        });
    }
    let bounds = model::union(&rects.values().copied().collect::<Vec<_>>()).unwrap_or(Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 });
    let people: Vec<Opt> = arr(&f, "roster").iter().filter(|r| s(r, "id") != "manager").map(|r| Opt { id: s(r, "id"), label: { let x = s(r, "name"); if x.is_empty() { "Unnamed".into() } else { x } } }).collect();
    let fit_now = with(cx, |e| { e.rects = rects; e.outs = outs_of; !e.fitted && e.cw > 0.0 });
    if fit_now { let (cw, ch) = with(cx, |e| (e.cw, e.ch)); let (z, px, py) = model::fit(bounds, cw, ch, 0.15, 0.15, 1.0); with(cx, |e| { e.z = z; e.px = px; e.py = py; e.fitted = true; }); }
    let (key, insp) = inspector(cx, &f, &validation);
    with(cx, |e| {
        if e.insp_key != key { e.insp_key = key.clone(); }
        e.nodes = nodes; e.edges = edges; e.people = people; e.insp = Some(insp);
    });
    push_ed(cx);
    with(cx, |e| { e.ed.bx = bounds.x; e.ed.by = bounds.y; e.ed.bw = bounds.w; e.ed.bh = bounds.h; });
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
fn inspector(cx: &mut App, f: &Value, validation: &Value) -> (String, Insp) {
    let (tab, sel, roster_sel, secrets) = with(cx, |e| (e.tab.clone(), e.sel.clone(), e.roster_sel.clone(), e.secrets.clone()));
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
            d.name = s(p, "name"); d.persona = s(p, "persona"); d.mentions_p = mentions(cx, &s(p, "persona"));
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
                d.question = s(dc, "question"); d.mentions_q = mentions(cx, &s(if ty == "ask" { &node["ask"] } else { dc }, if ty == "ask" { "prompt" } else { "question" }));
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
    d.name = s(f, "name"); d.description = s(f, "description"); d.mentions_d = mentions(cx, &s(f, "description"));
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

pub fn size(cx: &mut App, w: f32, h: f32) {
    let first = with(cx, |e| { let f = !e.fitted; e.cw = w; e.ch = h; f && e.state == "ok" });
    if first { render_model(cx); }
}
pub fn pan(cx: &mut App, dx: f32, dy: f32) { with(cx, |e| { e.px += dx; e.py += dy; }); push_ed(cx); }
pub fn zoom_at(cx: &mut App, d: f32, mx: f32, my: f32) {
    with(cx, |e| { let z2 = (e.z * (d * 0.0025).exp()).clamp(0.15, 2.0); e.px = mx - (mx - e.px) * z2 / e.z; e.py = my - (my - e.py) * z2 / e.z; e.z = z2; });
    push_ed(cx);
}
pub fn control(cx: &mut App, k: &str) {
    with(cx, |e| {
        let (cx, cy) = (e.cw / 2.0, e.ch / 2.0);
        if k == "fit" {
            if let Some(bx) = model::union(&e.rects.values().copied().collect::<Vec<_>>()) { let (z, px, py) = model::fit(bx, e.cw, e.ch, 0.15, 0.15, 1.0); e.z = z; e.px = px; e.py = py; }
        } else {
            let z2 = if k == "zoom-in" { (e.z * 1.2).min(2.0) } else { (e.z / 1.2).max(0.15) };
            e.px = cx - (cx - e.px) * z2 / e.z; e.py = cy - (cy - e.py) * z2 / e.z; e.z = z2;
        }
    });
    push_ed(cx);
}
pub fn mini(cx: &mut App, fx: f32, fy: f32) { with(cx, |e| { e.px = e.cw / 2.0 - fx * e.z; e.py = e.ch / 2.0 - fy * e.z; }); push_ed(cx); }

fn select(cx: &mut App, kind: &str, id: &str) {
    with(cx, |e| { e.sel = Some((kind.to_string(), id.to_string())); e.tab = "edit".into(); });
    render_model(cx);
}

pub fn node_down(cx: &mut App, id: &str) {
    let pos = with(cx, |e| arr(&e.f, "nodes").iter().find(|x| s(x, "id") == id).map(|x| (n(&x["position"], "x"), n(&x["position"], "y"))));
    let Some((x, y)) = pos else { return };
    with(cx, |e| { e.drag = Some((id.to_string(), x, y, e.f.clone(), false)); });
    select(cx, "node", id);
}
pub fn node_drag(cx: &mut App, id: &str, dx: f32, dy: f32) {
    let ok = with(cx, |e| {
        let z = e.z as f64;
        let Some((did, x0, y0, _, _)) = e.drag.clone() else { return false };
        if did != id { return false; }
        let (nx, ny) = (((x0 + dx as f64 / z) / 16.0).round() * 16.0, ((y0 + dy as f64 / z) / 16.0).round() * 16.0);
        if let Some(nodes) = e.f["nodes"].as_array_mut() { if let Some(nd) = nodes.iter_mut().find(|x| s(x, "id") == id) { nd["position"] = json!({ "x": nx, "y": ny }); } }
        if let Some(d) = e.drag.as_mut() { d.4 = true; }
        true
    });
    if ok { render_model(cx); }
}
pub fn node_up(_id: &str, moved: bool, cx: &mut App) {
    let changed = with(cx, |e| {
        let Some((_, _, _, snap, did)) = e.drag.take() else { return false };
        if !(moved && did) || snap == e.f { return false; }
        e.past.push(snap); if e.past.len() > 100 { e.past.remove(0); } e.future.clear(); e.dirty = true; e.save = "dirty".into(); e.last_key.clear();
        true
    });
    if changed { schedule_save(cx); render_model(cx); }
}

fn flow_of(cx: &mut App, x: f32, y: f32) -> (f32, f32) { with(cx, |e| ((x - e.px) / e.z, (y - e.py) / e.z)) }

pub fn handle_drag(cx: &mut App, node: &str, h: &str, x: f32, y: f32) {
    let (tx, ty) = flow_of(cx, x, y);
    let path = with(cx, |e| {
        let r = e.rects.get(node)?;
        let (sx, sy) = model::out_point(r, e.outs.get(node).map(|v| v.as_slice()).unwrap_or(&[]), h);
        Some(model::bezier(sx, sy, tx, ty, h == "note").0)
    });
    if let Some(p) = path { with(cx, |e| e.conn = Some(p)); push_ed(cx); }
}

pub fn handle_up(node: &str, h: &str, x: f32, y: f32, cx: &mut App) {
    let (fx, fy) = flow_of(cx, x, y);
    let target = with(cx, |e| {
        e.conn = None;
        let pad = 30.0 / e.z;
        e.rects.iter().filter(|(id, _)| id.as_str() != node).find(|(_, r)| fx >= r.x - pad && fx <= r.x + r.w + pad && fy >= r.y - pad && fy <= r.y + r.h + pad).map(|(id, _)| id.clone())
    });
    push_ed(cx);
    if let Some(t) = target { connect_ids(node, h, &t, cx); }
}

/// onConnect(): a valid connection is added; one that closes a loop gets a limit.
pub fn connect_ids(src: &str, h: &str, tgt: &str, cx: &mut App) -> bool {
    let th = if h == "note" { "notein" } else { "in" };
    let ok = with(cx, |e| model::can_connect(&e.f, src, h, tgt, th));
    if !ok { return false; }
    let looped = Rc::new(RefCell::new(false));
    let l2 = looped.clone();
    let (src, h, tgt) = (src.to_string(), h.to_string(), tgt.to_string());
    mutate("", cx, move |f| { *l2.borrow_mut() = model::connect(f, &src, &h, &tgt); });
    if *looped.borrow() { flash("That closes a loop, so it got a round limit of 3. Change it on the connection.", cx); }
    true
}

pub fn edge_click(cx: &mut App, id: &str) { select(cx, "edge", id); }
pub fn pane_click(cx: &mut App) {
    // The phone's sheet closes too.
    if with(cx, |e| { let was = e.sel.is_some() || (e.phone && !e.tab.is_empty()); e.sel = None; if e.phone { e.tab.clear(); } was }) { render_model(cx); }
}

pub fn key(k: &str, cx: &mut App) {
    if k != "delete" || with(cx, |e| e.run.is_some()) { return; }
    let sel = with(cx, |e| e.sel.clone());
    match sel {
        Some((kind, id)) if kind == "node" => {
            let deletable = with(cx, |e| arr(&e.f, "nodes").iter().any(|x| s(x, "id") == id && !["start", "manager", "end"].contains(&s(x, "type").as_str())));
            if deletable { mutate("", cx, |f| model::remove_nodes(f, &[id])); with(cx, |e| e.sel = None); render_model(cx); }
        }
        Some((_, id)) => { mutate("", cx, |f| model::remove_edge(f, &id)); with(cx, |e| e.sel = None); render_model(cx); }
        None => {}
    }
}

/// addAt(): a new step at a point of the canvas (the middle of the view when none).
fn add_at(kind: &str, at: Option<(f32, f32)>, cx: &mut App) {
    let (fx, fy) = match at { Some((x, y)) => flow_of(cx, x, y), None => { let (cw, ch) = with(cx, |e| (e.cw, e.ch)); flow_of(cx, cw / 2.0, ch / 2.0) } };
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
    with(cx, |e| {
        e.sel = Some(("node".into(), id.clone()));
        if kind == "employee:" { e.roster_add_to = Some(id.clone()); e.roster_sel = None; e.tab = "team".into(); } else { e.tab = if e.phone { String::new() } else { "edit".into() }; }
    });
    render_model(cx);
}

fn tidy(cx: &mut App) {
    let heights: HashMap<String, f32> = with(cx, |e| e.rects.iter().map(|(k, r)| (k.clone(), r.h)).collect());
    let pos = with(cx, |e| model::tidy(&e.f, &heights));
    mutate("", cx, move |f| {
        if let Some(nodes) = f["nodes"].as_array_mut() { for nd in nodes.iter_mut() { if let Some((x, y)) = pos.get(&s(nd, "id")) { nd["position"] = json!({ "x": x, "y": y }); } } }
    });
    control(cx, "fit");
}

fn pick_issue(cx: &mut App, key: &str) {
    if let Some(id) = key.strip_prefix("emp:") { with(cx, |e| { e.roster_sel = Some(id.to_string()); e.tab = "team".into(); }); render_model(cx); return; }
    if let Some(id) = key.strip_prefix("node:") {
        select(cx, "node", id);
        with(cx, |e| if let Some(r) = e.rects.get(id).copied() { let z = e.z.max(0.9); e.z = z; e.px = e.cw / 2.0 - (r.x + r.w / 2.0) * z; e.py = e.ch / 2.0 - (r.y + 40.0) * z; });
        push_ed(cx);
        return;
    }
    if let Some(id) = key.strip_prefix("edge:") { select(cx, "edge", id); }
}

fn remove_person(cx: &mut App) {
    let Some(id) = with(cx, |e| e.roster_sel.clone()) else { return };
    let (name, steps) = with(cx, |e| (arr(&e.f, "roster").iter().find(|r| s(r, "id") == id).map(|r| s(r, "name")).filter(|x| !x.is_empty()).unwrap_or_else(|| "this person".into()), model::steps_of(&e.f, &id)));
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
        with(cx, |e| e.roster_sel = None);
        render_model(cx);
    });
}

fn add_person(key: &str, cx: &mut App) {
    let dd = data();
    let tpl = arr(&dd, "personas").iter().find(|p| s(p, "key") == key).cloned();
    let skills: Vec<String> = known_skills(cx).into_iter().map(|x| x.0).collect();
    let emp = with(cx, |e| model::new_employee(tpl.as_ref(), &e.f, if skills.is_empty() { None } else { Some(skills.as_slice()) }));
    let id = s(&emp, "id");
    let add_to = with(cx, |e| e.roster_add_to.clone());
    let id2 = id.clone();
    mutate("", cx, move |f| {
        let mut r = arr(f, "roster").to_vec(); r.push(emp); f["roster"] = Value::Array(r);
        // The step it was added for now names them.
        if let Some(nid) = add_to { if let Some(nd) = node_mut(f, &nid) { nd["employeeId"] = json!(id2); } }
    });
    with(cx, |e| e.roster_sel = Some(id));
    render_model(cx);
}

pub fn act(a: &str, x: &str, y: &str, cx: &mut App) {
    match a {
        "e-back" => save_now(cx, |_, cx| super::close_sub(cx)),
        "e-save" => { with(cx, |e| e.dirty = true); save_now(cx, |_, _| {}); }
        "e-reload" => reload(cx),
        "e-undo" => undo(cx),
        "e-redo" => redo(cx),
        "e-tidy" => if with(cx, |e| e.run.is_none()) { tidy(cx) },
        "e-dry-start" => dry_start(cx),
        "e-dry-end" => { with(cx, |e| { e.run = None; e.dry_log.clear(); e.dry_err.clear(); e.dry_answers.clear(); e.dry_notes.clear(); }); render_model(cx); }
        "e-dry-handoff" => dry_send(x, y, cx),
        "e-dry-answer" => dry_answer(x, y, cx),
        "e-dry-reply" => dry_reply(x, y, cx),
        "e-dry-wait" => dry_wait(x, y, cx),
        "e-dry-same" => { with(cx, |e| e.dry_same = !e.dry_same); render_model(cx); }
        "e-settings" => { with(cx, |e| { e.sel = None; e.tab = "edit".into(); }); render_model(cx); }
        "e-tab" => { with(cx, |e| e.tab = x.to_string()); render_model(cx); }
        "e-add" => add_at(x, None, cx),
        "e-add-at" => {
            let p: Vec<f32> = y.split(',').map(|v| v.trim().parse::<f32>().unwrap_or(-1.0)).collect();
            let (px, py) = (p.first().copied().unwrap_or(-1.0), p.get(1).copied().unwrap_or(-1.0));
            let (cw, ch) = with(cx, |e| (e.cw, e.ch));
            if px >= 0.0 && py >= 0.0 && px <= cw && py <= ch { add_at(x, Some((px, py)), cx); }
        }
        "e-delete" => key("delete", cx),
        "e-person" => {
            let id = if x.is_empty() { with(cx, |e| e.sel.clone().and_then(|(k, id)| if k == "node" { arr(&e.f, "nodes").iter().find(|n2| s(n2, "id") == id).map(|n2| s(n2, "employeeId")) } else { None })).unwrap_or_default() } else { x.to_string() };
            with(cx, |e| { e.roster_sel = Some(id); e.tab = "team".into(); });
            render_model(cx);
        }
        "e-team" => { with(cx, |e| { e.roster_sel = None; e.roster_add_to = None; }); render_model(cx); }
        "e-add-person" => add_person(x, cx),
        "e-remove-person" => remove_person(cx),
        "e-issue" => pick_issue(cx, x),
        "e-cr-open" => with(cx, |e| e.cr_open = !e.cr_open),
        "e-more" => with(cx, |e| e.more_open = !e.more_open),
        _ => {}
    }
}

// ─── The fields ───

fn clamp_num(v: &str, lo: i64, hi: i64) -> i64 { let t = v.trim(); if t.is_empty() { 0 } else { t.parse::<f64>().map(|x| (x.round() as i64).clamp(lo, hi)).unwrap_or(lo) } }
fn list_of(v: &str, sep: char) -> Value { json!(v.split(sep).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect::<Vec<_>>()) }
fn set_in(f: &mut Value, path: &[&str], v: Value) { let mut cur = f; for p in &path[..path.len() - 1] { if !cur[*p].is_object() { cur[*p] = json!({}); } cur = &mut cur[*p]; } cur[path[path.len() - 1]] = v; }

/// A field of the pane changed (Inspector.jsx's set()).
pub fn edit(k: &str, v: &str, cx: &mut App) {
    // The Dry run's notes (not the factory).
    if let Some(nk) = k.strip_prefix("dry-note:") { with(cx, |e| { e.dry_notes.insert(nk.to_string(), json!(v)); }); return; }
    let (sel, roster_sel, tab) = with(cx, |e| (e.sel.clone(), e.roster_sel.clone(), e.tab.clone()));
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
                let emps: Vec<String> = with(cx, |e| arr(&e.f, "roster").iter().filter(|r| s(r, "id") != "manager").map(|r| s(r, "id")).collect());
                if i == emps.len() + 1 { with(cx, |e| { e.roster_add_to = Some(id.clone()); e.roster_sel = None; e.tab = "team".into(); }); render_model(cx); return; }
                let eid = if i == 0 { String::new() } else { emps.get(i - 1).cloned().unwrap_or_default() };
                mutate("", cx, move |f| { if let Some(nd) = node_mut(f, &id2) { nd["employeeId"] = json!(eid); } });
                return;
            }
            let roster_ids: Vec<String> = with(cx, |e| arr(&e.f, "roster").iter().map(|r| s(r, "id")).collect());
            let forks: Vec<String> = with(cx, |e| arr(&e.f, "nodes").iter().filter(|x| s(x, "type") == "decision" && s(&x["decision"], "kind") == "many").map(|x| s(x, "id")).collect());
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

pub fn props(cx: &mut App) -> Value {
    let fx_slash = parts::pwith(cx, |p| p.slash.as_ref().map(|m| json!({ "field": m.key.trim_start_matches("ed|"), "items": m.items.iter().map(|x| x.0.clone()).collect::<Vec<_>>(), "at": m.at })).unwrap_or(Value::Null));
    let toast = w::toast_text(cx).to_string();
    let mut out = with(cx, |e| {
        let ed = &e.ed;
        json!({
            "state": ed.state, "error": ed.error, "name": ed.name, "save": ed.save, "saveLabel": ed.save_label, "draft": ed.draft,
            "canUndo": ed.can_undo, "canRedo": ed.can_redo, "tab": ed.tab, "issuesLabel": ed.issues_label, "zoom": e.z, "conn": ed.conn,
            "slash": fx_slash.clone(),
            "id": e.id, "sel": e.sel.clone().map(|x| format!("{}:{}", x.0, x.1)).unwrap_or_default(),
            "nodes": e.nodes.iter().map(|n2| json!({ "id": n2.id, "kind": n2.kind, "title": n2.title, "body": n2.body, "x": n2.x, "y": n2.y, "sel": n2.sel, "badge": n2.badge, "outs": n2.outs.iter().map(|o| json!({ "id": o.id, "label": o.label })).collect::<Vec<_>>() })).collect::<Vec<_>>(),
            "edges": e.edges.iter().map(|x| json!({ "id": x.id, "tone": x.tone, "label": x.label, "sel": x.sel })).collect::<Vec<_>>(),
            "people": e.people.iter().map(|p| p.label.clone()).collect::<Vec<_>>(),
            "pane": e.insp.as_ref().map(|d| json!({ "mode": d.mode, "kindLabel": d.kind_label, "label": d.label, "name": d.name, "issues": d.issues.iter().map(|i| i.message.clone()).collect::<Vec<_>>(),
                "people": d.people.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), "dkind": d.dkind_i, "options": d.options.iter().map(|o| o.label.clone()).collect::<Vec<_>>(), "rounds": d.rounds,
                "issuesTitle": d.issues_title, "persona": d.persona, "edgeDesc": d.edge_desc })).unwrap_or(Value::Null),
        })
    });
    // The editor's word (a loop's limit): the shared toast's, while it shows.
    out["toast"] = json!(toast);
    out["dry"] = dry_props(cx);
    out
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
        node_down: Box::new(|id, cx| node_down(cx, id)),
        node_drag: Box::new(|id, dx, dy, cx| node_drag(cx, id, dx, dy)),
        node_up: Box::new(|id, moved, cx| node_up(id, moved, cx)),
        handle_drag: Box::new(|nd, h, x, y, cx| handle_drag(cx, nd, h, x, y)),
        handle_up: Box::new(|nd, h, x, y, cx| handle_up(nd, h, x, y, cx)),
        edge: Box::new(|id, cx| edge_click(cx, id)),
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
fn line(k: &str, v: &str, ph: &str, mono: bool, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let k2 = k.to_string();
    parts::line(&mut fx.p, &format!("ed|{k}"), v, ph, mono, false, parts::Commit::Change, window, cx, move |t, cx| set(&k2, &t, cx))
}
fn area(k: &str, v: &str, ph: &str, rows: usize, mono: bool, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let k2 = k.to_string();
    parts::area(&mut fx.p, &format!("ed|{k}"), v, ph, rows, mono, window, cx, move |t, cx| set(&k2, &t, cx))
}
/// A SkillText: "/" offers the machine's skills; the skills it names under it.
fn skill_text(k: &str, v: &str, ph: &str, rows: usize, mentions: &[Check], fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> Div {
    let k2 = k.to_string();
    let skills: Vec<(String, String)> = { let mut out: Vec<(String, String)> = vec![]; for (nm, d, _) in &fx.e.skills { if !out.iter().any(|x| x.0 == *nm) { out.push((nm.clone(), d.clone())); } } out };
    div().flex().flex_col().gap(T::SP_4)
        .child(parts::skill_area(&mut fx.p, &format!("ed|{k}"), v, ph, rows, skills, window, cx, move |t, cx| set(&k2, &t, cx)))
        .child(parts::mentions(mentions))
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

fn choices(key: &'static str, items: &[Opt], min: usize, add: &str, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> Div {
    let mut d = div().flex().flex_col().gap(T::SP_6);
    for (i, it) in items.iter().enumerate() {
        let (id, id2) = (it.id.clone(), it.id.clone());
        let k = format!("{key}-label:{id}");
        d = d.child(div().flex().items_center().gap(T::SP_4)
            .child(div().flex_1().min_w_0().child(line(&k, &it.label, &format!("Choice {}", i + 1), false, fx, window, cx)))
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

fn node_insp(d: &Insp, p: Stateful<Div>, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> Stateful<Div> {
    let mut p = issues(p.child(pane_title(&d.kind_label, false, d.can_delete, "Delete", "e-delete")), &d.issues, false);
    p = p.child(field("Label", "", "Shown on the canvas; also how the team refers to this step.", line("label", &d.label, &d.label_ph, false, fx, window, cx)));
    match d.kind.as_str() {
        "employee" => {
            p = p.child(field("Who does this step", "", "", pick("e-who", d.who.clone(), d.who_i, "who")));
            if d.has_person { p = p.child(div().child(btn("e-persona", "Edit their persona", Some(IconName::User), "ghost", true, false).on_click(a0("e-person")))); }
        }
        "manager" => { p = p.child(div().child(btn("e-mpersona", "Edit the Manager's persona", Some(IconName::Crown), "ghost", true, false).on_click(a("e-person", "manager".into())))); }
        "decision" => {
            p = p.child(field("Question", "", "Answered at handoff. You can name skills with /skill-name.", skill_text("question", &d.question, "Does it need a UI change?", 2, &d.mentions_q, fx, window, cx)))
                .child(field("Kind of answer", "", "", pick("e-dkind", vec!["Yes or no".into(), "Pick one".into(), "Pick any (runs them in parallel)".into()], d.dkind_i, "dkind")));
            if d.dkind_i != 0 { p = p.child(field("Options", "", "", choices("options", &d.options, 2, "Add option", fx, window, cx))); }
            if d.dkind_i == 0 {
                p = p.child(tick("e-review", "This is a review gate (approval)", d.review, true, "review".into()));
                if d.review { p = p.child(note("hint", "\"No\" needs at least one open blocking finding saying what must change; \"yes\" needs none left open. Findings carry over between rounds, so reviewers check last round's requests first, and nits become follow-ups instead of blocking.")); }
            }
            p = p.child(field("Who answers", "", "Usually whoever arrives here. After a Join or an Ask me, someone has to be named.", pick("e-answered", d.answered.clone(), d.answered_i, "answered")));
        }
        "ask" => {
            p = p.child(field("What you are asked", "", "You get a notification and the run waits for your reply.", skill_text("prompt", &d.prompt, "Ship it?", 2, &d.mentions_q, fx, window, cx)))
                .child(field("Replies", "", "", choices("replies", &d.replies, 1, "Add reply", fx, window, cx)));
        }
        "wait" => {
            p = p.child(field("Wait for", "", "", pick("e-wkind", vec!["A command to succeed (CI, a deploy, a health check)".into(), "A signal (a URL called by CI or a hook, or you)".into(), "A length of time".into(), "A time of day".into()], d.wkind_i, "wkind")));
            if d.wkind_i == 2 { p = p.child(field("Minutes", "", "", line("minutes", &d.minutes, "", false, fx, window, cx))); }
            if d.wkind_i == 3 { p = p.child(field("Until (this server's time)", "", "The next time the clock says this; tomorrow if it has passed today.", line("wat", &d.at, "07:30", false, fx, window, cx))); }
            if d.wkind_i == 0 {
                p = p.child(field("Command", "", "Run in the run's worktree; the wait is over when it exits 0. E.g. gh run watch --exit-status, curl -fs localhost:4310/health, npm test.", line("command", &d.command, "gh run watch --exit-status", true, fx, window, cx)))
                    .child(field("Try every (seconds)", "", "", line("every", &d.every, "", false, fx, window, cx)));
            }
            if d.wkind_i == 1 { p = p.child(field("Waiting for", "", "Shown on the live graph. Each run gets a secret URL to POST to; you can also end the wait from the run.", line("what", &d.what, "The staging deploy to finish", false, fx, window, cx))); }
            if d.wkind_i <= 1 { p = p.child(field("Give up after (minutes)", "", "Then it goes down Timed out, so the run never waits forever.", line("timeout", &d.timeout, "", false, fx, window, cx))); }
            p = p.child(note("hint", "What came in (the last summary, any note) goes on to whoever is next, with what happened during the wait. Waits survive a restart."));
        }
        "join" => { p = p.child(field("Joins the branches of", "", "The pick-any decision whose parallel branches meet here.", pick("e-fork", d.forks.clone(), d.fork_i, "fork"))); }
        "start" => { p = p.child(note("hint", "The task you give when starting the factory enters here and goes to the Manager.")); }
        "end" => { p = p.child(note("hint", "Only the Manager's Finish reaches here. The Manager then presents the result and you decide whether to merge it.")); }
        _ => {}
    }
    p
}

fn edge_insp(d: &Insp, p: Stateful<Div>, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> Stateful<Div> {
    let mut p = p.child(pane_title(if d.note { "Note" } else { "Connection" }, false, d.can_delete, "Delete", "e-delete")).child(note("hint", d.edge_desc.clone()));
    p = issues(p, &d.issues, false);
    if d.note {
        p = p.child(note("hint", d.label.clone())).child(field("What the note is for", "", "", line("note-label", &d.note_label, "Instructions for reviewers", false, fx, window, cx)));
    } else {
        p = p.child(field("Round limit", "", "Set it on the connection that goes back round a loop. When it runs out, the Manager takes over. Empty means no limit.", line("rounds", &d.rounds, "No limit", false, fx, window, cx)))
            .child(tick("e-reqnote", "Require a note on this route", d.require_note, true, "require-note".into()));
        if d.require_note { p = p.child(field("What the note says", "", "", line("note-label", &d.note_label, "What to change", false, fx, window, cx))); }
    }
    p
}

fn pair(a: impl IntoElement, b: impl IntoElement) -> Div { div().flex().gap(T::SP_8).child(div().flex_1().min_w_0().child(a)).child(div().flex_1().min_w_0().child(b)) }

fn settings_insp(d: &Insp, p: Stateful<Div>, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> Stateful<Div> {
    let cr_open = re(fx, |e| e.cr_open);
    let mut p = p.child(pane_title("Factory", false, false, "", ""))
        .child(field("Name", "", "", line("name", &d.name, "", false, fx, window, cx)))
        .child(field("Description", "", "What this factory is for. /skill-name works here too.", skill_text("description", &d.description, "", 3, &d.mentions_d, fx, window, cx)))
        .child(w::disclosure("e-cr-open", "Change request form", cr_open).on_click(a0("e-cr-open")));
    if cr_open {
        let mut c = div().flex().flex_col().gap(T::SP_8).child(note("hint", "What the person fills in to start a run. The Manager gets all of it; every step gets the title, the goal and the \"done\" checks. The title and attachments are always there."));
        let total = d.cr.len();
        for (i, sec) in d.cr.iter().enumerate() {
            let is = i.to_string();
            let (i1, i2, i3) = (is.clone(), is.clone(), is.clone());
            c = c.child(div().flex().flex_col().gap(T::SP_6).p(T::SP_8).rounded(T::R_FIELD).border_1().border_color(T::border()).bg(T::bg_card())
                .child(div().flex().items_center().gap(T::SP_4)
                    .child(div().flex_1().min_w_0().child(line(&format!("cr-label:{i}"), &sec.label, "Section name", false, fx, window, cx)))
                    .child(w::disabled(w::icon_button(("e-cr-up", i), IconName::ChevronUp, "Move up"), i == 0).on_click(move |_, _, cx| set("cr-up", &i1, cx)))
                    .child(w::disabled(w::icon_button(("e-cr-down", i), IconName::ChevronDown, "Move down"), i + 1 >= total).on_click(move |_, _, cx| set("cr-down", &i2, cx)))
                    .child(w::disabled(w::icon_button(("e-cr-rm", i), IconName::Trash, "Remove"), total <= 1).on_click(move |_, _, cx| set("cr-remove", &i3, cx))))
                .child(line(&format!("cr-hint:{i}"), &sec.hint, "Hint: what to write here", false, fx, window, cx))
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
            .child(field("More rules for everyone", "", "One per line, Claude Code style: Bash(docker compose up:*), WebFetch(domain:docs.example.com). Too broad or forbidden rules are ignored.", area("rules", &d.rules, "", 3, true, fx, window, cx)))
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
        .child(field("Setup command", "", "Runs in each new worktree before the team starts, e.g. npm ci.", line("setup", &d.setup, "npm ci", true, fx, window, cx)))
        .child(pair(field("Max handoffs", "", "", line("max-handoffs", &d.max_handoffs, "", false, fx, window, cx)), field("Max minutes", "", "", line("max-minutes", &d.max_minutes, "", false, fx, window, cx))))
        .child(note("hint", "When a limit is reached the run waits for you; you can let it carry on."))
        .child(field("Manager check-in every (minutes)", "", &d.tick_hint, line("tick", &d.tick, "The Manager chooses", false, fx, window, cx)))
        .child(sub("USAGE"))
        .child(note("hint", "Your Claude usage is shared with your own chats. These keep a run inside what you choose; each can also be changed on a running run."))
        .child(pair(field("5-hour, pause at %", "", "", line("l5h", &d.l5h, "", false, fx, window, cx)), field("Weekly, pause at %", "", "", line("lweek", &d.lweek, "", false, fx, window, cx))))
        .child(note("hint", d.lim_hint.clone()))
        .child(pair(field("People at once", "", "0: no limit", line("lpar", &d.lpar, "", false, fx, window, cx)), field("One at a time above %", "", "Of the 5-hour. 0: off", line("lone", &d.lone, "", false, fx, window, cx))))
        .child(note("hint", "Parallel steps beyond the limit wait their turn instead of all running together, which spreads the load. Each person's model is in the Team panel: a cheaper model for routine roles stretches usage too."))
}

fn team_insp(d: &Insp, p: Stateful<Div>) -> Stateful<Div> {
    let mut list = div().flex().flex_col().gap(T::SP_4);
    for (i, pp) in d.people.iter().enumerate() {
        list = list.child(w::list_row(("e-person", i), if pp.manager { IconName::Crown } else { IconName::User }, pp.name.clone(), pp.meta.clone(), "", false)
            .on_click(a("e-person", pp.id.clone())));
    }
    let mut pills = div().flex().flex_wrap().gap(T::SP_6);
    let dd = data();
    for (i, o) in d.personas.iter().enumerate() {
        // Its title: the persona it starts from.
        let persona = arr(&dd, "personas").iter().find(|p| s(p, "key") == o.id && !o.id.is_empty()).map(|p| s(p, "persona")).unwrap_or_default();
        let b2 = btn(("e-tpl", i), &o.label, Some(IconName::Plus), "", true, false);
        let b2 = if persona.is_empty() { b2 } else { b2.tooltip(persona) };
        pills = pills.child(b2.on_click(a("e-add-person", o.id.clone())));
    }
    p.child(pane_title("Team", false, false, "", "")).child(list).child(sub("ADD SOMEONE")).child(pills)
        .child(note("hint", "Templates are starting points: change the persona however you like."))
}

fn person_insp(d: &Insp, p: Stateful<Div>, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> Stateful<Div> {
    let more = re(fx, |e| e.more_open);
    let mut p = issues(p.child(pane_title(if d.manager { "Manager" } else { "Employee" }, true, !d.manager, "Remove from the team", "e-remove-person")), &d.issues, false);
    p = p.child(field("Name", "", "", line("p-name", &d.name, "", false, fx, window, cx)))
        .child(field("Persona", "", "Who they are and how they work. They read it and act as it says. /skill-name names a skill they should use.", skill_text("persona", &d.persona, "", 9, &d.mentions_p, fx, window, cx)))
        .child(pair(field("Model", "", "", pick("e-model", vec!["Default".into(), "Opus".into(), "Sonnet".into(), "Haiku".into()], d.model_i, "model")),
            field("Effort", "", "", pick("e-effort", ["Default", "low", "medium", "high", "xhigh", "max"].iter().map(|x| x.to_string()).collect(), d.effort_i, "effort"))));
    if !d.manager {
        p = p.child(field("Memory", "", &d.memory_hint, pick("e-memory", vec!["Keeps memory".into(), "Fresh each time".into()], d.memory_i, "memory")))
            .child(tick("e-canedit", "Can edit files", d.can_edit, true, "can-edit".into()));
    }
    p = p.child(field("Pinned skills", "", "Comma separated. Same as naming them in the persona.", line("pinned", &d.pinned, "frontend-design, run", false, fx, window, cx)));
    if d.show_presets { p = p.child(field("This person may also, without asking", "", "On top of what everyone may (factory settings).", checks("p-preset", &d.p_presets))); }
    p = p.child(w::disclosure("e-more", "More tool rules", more).on_click(a0("e-more")));
    if more {
        p = p.child(field("Always allowed (this person only)", "", "Claude Code rules, comma separated, e.g. Bash(npm test:*).", line("allow", &d.allow, "", true, fx, window, cx)))
            .child(field("Never allowed", "", "", line("deny", &d.deny, "", true, fx, window, cx)));
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

// ─── The Dry run (DryRun.jsx) ───

/// apply(): a step of the engine; its refusal shows, or the run moves on and the log says what happened.
fn dry_apply(res: Value, what: String, cx: &mut App) {
    if !b(&res, "ok") { let er = s(&res, "error"); with(cx, |e| e.dry_err = er); render_model(cx); return; }
    let names: HashMap<String, String> = with(cx, |e| arr(&e.f, "roster").iter().map(|r| (s(r, "id"), s(r, "name"))).collect());
    let lines: Vec<String> = arr(&res, "effects").iter().map(|x| format!("  \u{2192} {}", describe_effect(x, &names))).collect();
    with(cx, |e| {
        e.dry_err.clear();
        e.run = Some(res["run"].clone());
        e.dry_answers.clear(); e.dry_notes.clear();
        e.dry_log.push(what); e.dry_log.extend(lines);
        let n2 = e.dry_log.len(); if n2 > 80 { e.dry_log.drain(..n2 - 80); }
    });
    parts::pwith(cx, |p| parts::forget(p, "ed|dry-"));
    render_model(cx);
}

/// describe(): an effect in words.
fn describe_effect(x: &Value, names: &HashMap<String, String>) -> String {
    let who = |k: &str| names.get(&s(x, k)).cloned().unwrap_or_else(|| "undefined".into());
    match s(x, "type").as_str() {
        "activate" => if s(x, "mode") == "answer" { format!("{} is asked to decide", who("employeeId")) } else { format!("{} gets the work", who("employeeId")) },
        "workspace" => format!("a separate worktree is made for \"{}\"", s(x, "name")),
        "merge" => if b(x, "abort") { "the branches are stopped and their work merged back".into() } else { "the branches are merged back together".into() },
        "cancel" => "the other branches are stopped".into(),
        "ask" => format!("you are asked: {}", s(x, "prompt")),
        "wait" => "the flow waits".into(),
        "finish" => "the Manager finishes and presents the result".into(),
        "cap" => format!("the run reaches its limit of {} handoffs and waits for you", x.get("limit").map(|l| l.to_string()).unwrap_or_default()),
        t => t.to_string(),
    }
}

fn dry_start(cx: &mut App) {
    let (f, ok) = with(cx, |e| (super::valid::normalize(&e.f), b(&e.validation, "ok")));
    if !ok { return; }
    let r = super::engine::create_run("dry", "dry", &f, "A dry run");
    let res = super::engine::start(&r);
    with(cx, |e| { e.dry_log.clear(); e.dry_tree = 0; });
    dry_apply(res, "Started".into(), cx);
}

/// send(): whoever has `token` hands off (extra: "" | finish | escalate).
fn dry_send(token: &str, extra: &str, cx: &mut App) {
    let Some(run) = with(cx, |e| e.run.clone()) else { return };
    let req = super::engine::requirements(&run, token);
    let who_id = s(&req, "who");
    let who = with(cx, |e| arr(&e.f, "roster").iter().find(|r| s(r, "id") == who_id).map(|r| s(r, "name"))).unwrap_or(who_id);
    let (answers, notes, same, tree) = with(cx, |e| { e.dry_tree += 1; (e.dry_answers.clone(), e.dry_notes.clone(), e.dry_same, e.dry_tree) });
    let mut payload = json!({ "summary": "Dry run step", "answers": answers, "notes": notes });
    if extra == "finish" { payload["finish"] = json!(true); }
    if extra == "escalate" { payload["escalate"] = json!("Stuck in the dry run"); }
    let res = super::engine::handoff(&run, token, &payload, &if same { "same".to_string() } else { format!("tree{tree}") });
    dry_apply(res, format!("{who} handed off{}", match extra { "finish" => " (finish)", "escalate" => " (escalated)", _ => "" }), cx);
}

/// A decision's answer (toggle()); "" with `none`: the pick-any's None.
fn dry_answer(decision: &str, h: &str, cx: &mut App) {
    let Some(run) = with(cx, |e| e.run.clone()) else { return };
    // The routes of everyone with work now: the decision is in one of them.
    let routes: Vec<Value> = arr(&run, "tokens").iter().filter(|t| ["work", "answer"].contains(&s(t, "status").as_str())).map(|t| super::engine::requirements(&run, &s(t, "id"))["route"].clone()).collect();
    let dc = find_decision(&routes, decision);
    let many = dc.as_ref().map(|d| s(d, "kind") == "many").unwrap_or(false);
    let last = dc.map(|d| d["lastPicks"].clone()).unwrap_or(Value::Null);
    with(cx, |e| {
        if !many { e.dry_answers.insert(decision.to_string(), json!(h)); return; }
        if h.is_empty() { e.dry_answers.insert(decision.to_string(), json!([])); return; }
        let cur = e.dry_answers.get(decision).cloned().filter(|v| v.is_array()).or_else(|| last.is_array().then(|| last.clone())).unwrap_or(json!([]));
        let mut l: Vec<String> = cur.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        if let Some(i) = l.iter().position(|x| x == h) { l.remove(i); } else { l.push(h.to_string()); }
        e.dry_answers.insert(decision.to_string(), json!(l));
    });
    render_model(cx);
}

fn find_decision(routes: &[Value], id: &str) -> Option<Value> {
    fn walk(item: &Value, id: &str) -> Option<Value> {
        let d = &item["decision"];
        if d.is_object() {
            if s(d, "id") == id { return Some(d.clone()); }
            for br in arr(d, "branches") { if let Some(x) = walk(&br["next"], id) { return Some(x); } }
        }
        None
    }
    routes.iter().find_map(|r| walk(r, id))
}

fn dry_reply(token: &str, reply: &str, cx: &mut App) {
    let Some(run) = with(cx, |e| e.run.clone()) else { return };
    let (label, note) = with(cx, |e| {
        let node = arr(&run, "tokens").iter().find(|t| s(t, "id") == token).map(|t| s(t, "nodeId")).and_then(|nid| arr(&e.f, "nodes").iter().find(|x| s(x, "id") == nid).cloned()).unwrap_or(Value::Null);
        (arr(&node["ask"], "replies").iter().find(|r| s(r, "id") == reply).map(|r| s(r, "label")).unwrap_or_default(), e.dry_notes.get("__ask").and_then(|v| v.as_str()).unwrap_or("").to_string())
    });
    dry_apply(super::engine::ask_reply(&run, token, reply, &note), format!("You replied \"{label}\""), cx);
}

fn dry_wait(token: &str, outcome: &str, cx: &mut App) {
    let Some(run) = with(cx, |e| e.run.clone()) else { return };
    let two = with(cx, |e| arr(&run, "tokens").iter().find(|t| s(t, "id") == token).map(|t| s(t, "nodeId")).and_then(|nid| arr(&e.f, "nodes").iter().find(|x| s(x, "id") == nid).map(|x| { let k = s(&x["wait"], "kind"); k == "command" || k == "signal" })).unwrap_or(false));
    let what = if outcome == "timeout" { "The wait timed out" } else if two { "The wait was done" } else { "The time passed" };
    dry_apply(super::engine::wait_done(&run, token, outcome, "in the dry run"), what.into(), cx);
}

fn dry_insp(d: &Insp, mut p: Stateful<Div>, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> Stateful<Div> {
    let Some(run) = fx.e.run.clone() else {
        return p.child(pane_title("Dry run", false, false, "", ""))
            .child(note("hint", "Walk through the flow by hand, answering for each person, before spending anything on Claude. The canvas shows where the work is and how many rounds each loop has used."))
            .when(!d.ok, |p| p.child(parts::issue("e-dry-bad", "error", "Fix the problems first: a factory that cannot run cannot be walked either.", "")))
            .child(div().child(btn("e-dry", "Start a dry run", Some(IconName::Play), "primary", d.ok, false).on_click(a0("e-dry-start"))));
    };
    let f = fx.e.f.clone();
    let roster: HashMap<String, String> = arr(&f, "roster").iter().map(|r| (s(r, "id"), s(r, "name"))).collect();
    let status = s(&run, "status");
    p = p.child(div().flex().items_center().gap(T::SP_8)
        .child(div().flex_1().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).child("Dry run"))
        .child(btn("e-dry-end", "End", Some(IconName::X), "ghost", true, false).on_click(a0("e-dry-end"))))
        .child(note("hint", format!("Handoffs: {}{}", n(&run, "handoffs") as i64, if status == "waiting" { " · waiting for you" } else { "" })));
    if status == "finishing" {
        p = p.child(div().flex().items_center().gap(T::SP_6).text_color(T::green()).text_size(T::TEXT_13).child(w::icon(IconName::Check, T::ICON_MD)).child("Finished: the Manager would now present the result."));
    }
    let active: Vec<Value> = arr(&run, "tokens").iter().filter(|t| ["work", "answer", "ask", "wait"].contains(&s(t, "status").as_str())).cloned().collect();
    for (i, t) in active.iter().enumerate() { p = p.child(dry_actor(i, t, &run, &f, &roster, fx, window, cx)); }
    let (err, log) = (fx.e.dry_err.clone(), fx.e.dry_log.clone());
    if !err.is_empty() { p = p.child(parts::issue("e-dry-err", "error", &err, "")); }
    if !log.is_empty() {
        p = p.child(div().id("e-dry-log").max_h(T::FACTORIES_MINI_W).overflow_y_scroll().p(T::SP_8).rounded(T::R_CONTROL).border_1().border_color(T::border()).bg(T::bg())
            .font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_secondary()).child(log.join("\n")));
    }
    p
}

/// Actor(): the one with the work (a wait, a question for you, or a person).
#[allow(clippy::too_many_arguments)]
fn dry_actor(i: usize, t: &Value, run: &Value, f: &Value, roster: &HashMap<String, String>, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> Div {
    let node = arr(f, "nodes").iter().find(|x| s(x, "id") == s(t, "nodeId")).cloned().unwrap_or(Value::Null);
    let tid = s(t, "id");
    let head = |icon: IconName, text: String| div().flex().items_center().gap(T::SP_6).font_weight(T::W_SEMIBOLD).text_size(T::TEXT_13).child(w::icon(icon, T::ICON_MD)).child(text);
    let card = w::card().p(T::SP_10).gap(T::SP_8);
    match s(t, "status").as_str() {
        "wait" => {
            let wt = &node["wait"];
            let two = { let k = s(wt, "kind"); k == "command" || k == "signal" };
            let l = s(&node, "label");
            let (t1, t2) = (tid.clone(), tid.clone());
            card.child(head(IconName::Clock, format!("{}: {}", if l.is_empty() { "Wait".to_string() } else { l }, model::wait_text(wt))))
                .child(div().flex().flex_wrap().gap(T::SP_6)
                    .child(btn(("e-dry-done", i), if two { "Done" } else { "Time passes" }, None, "", true, false).on_click(move |_, _, cx| fact("e-dry-wait", &t1, "done", cx)))
                    .when(two, |d| d.child(btn(("e-dry-timeout", i), "Timed out", None, "", true, false).on_click(move |_, _, cx| fact("e-dry-wait", &t2, "timeout", cx)))))
        }
        "ask" => {
            let note_v = fx.e.dry_notes.get("__ask").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let mut row = div().flex().flex_wrap().gap(T::SP_6);
            for (j, r) in arr(&node["ask"], "replies").iter().enumerate() {
                let (t1, rid) = (tid.clone(), s(r, "id"));
                row = row.child(btn(("e-dry-reply", i * 100 + j), &s(r, "label"), None, "", true, false).on_click(move |_, _, cx| fact("e-dry-reply", &t1, &rid, cx)));
            }
            card.child(head(IconName::MessageCircleQuestionMark, format!("You are asked: {}", s(&node["ask"], "prompt"))))
                .child(area("dry-note:__ask", &note_v, "Optional note", 2, false, fx, window, cx))
                .child(row)
        }
        st => {
            let req = super::engine::requirements(run, &tid);
            let who_id = s(&req, "who");
            let who = roster.get(&who_id).cloned().unwrap_or_else(|| who_id.clone());
            let l = s(&node, "label");
            let mut c = card.child(head(if who_id == "manager" { IconName::Crown } else { IconName::User },
                format!("{who} {}{}", if st == "answer" { "is asked to decide" } else { "has the work" }, if !l.is_empty() && l != who { format!(" · {l}") } else { String::new() })));
            let route = &req["route"];
            if route.is_object() { c = c.child(dry_route(route, fx, window, cx)); } else { c = c.child(note("hint", "No decisions here.")); }
            for nt in arr(&req, "noteTargets") {
                let k = s(nt, "key");
                let v = fx.e.dry_notes.get(&k).and_then(|v| v.as_str()).unwrap_or("").to_string();
                let lb = s(nt, "label");
                c = c.child(line(&format!("dry-note:{k}"), &v, &format!("Optional note for {}{}", s(nt, "to"), if lb.is_empty() { String::new() } else { format!(" ({lb})") }), false, fx, window, cx));
            }
            let (t1, t2, t3) = (tid.clone(), tid.clone(), tid.clone());
            c = c.child(div().flex().flex_wrap().gap(T::SP_6)
                .child(btn(("e-dry-hand", i), "Hand off", None, "primary", true, false).on_click(move |_, _, cx| fact("e-dry-handoff", &t1, "", cx)))
                .when(b(&req, "canFinish"), |d| d.child(btn(("e-dry-finish", i), "Finish", None, "", true, false).on_click(move |_, _, cx| fact("e-dry-handoff", &t2, "finish", cx))))
                .when(s(&req, "mode") == "work" && who_id != "manager", |d| d.child(btn(("e-dry-esc", i), "Escalate", None, "ghost", true, false).on_click(move |_, _, cx| fact("e-dry-handoff", &t3, "escalate", cx)))));
            if who_id != "manager" && st == "work" {
                c = c.child(w::check(("e-dry-same", i), "Pretend nothing changed in the code (tests the no-progress check)", fx.e.dry_same, |_, _, cx| fact("e-dry-same", "", "", cx)));
            }
            c
        }
    }
}

/// Route(): a route's note, then where it goes, or its question and the routes of the answers chosen.
fn dry_route(item: &Value, fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let mut d = div().flex().flex_col().gap(T::SP_6);
    if b(item, "requireNote") {
        let k = s(item, "edgeId");
        let v = fx.e.dry_notes.get(&k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let nl = s(item, "noteLabel");
        d = d.child(line(&format!("dry-note:{k}"), &v, &format!("Required: {}", if nl.is_empty() { "a note".to_string() } else { nl }), false, fx, window, cx));
    }
    let dc = &item["decision"];
    if !dc.is_object() {
        let to = &item["to"];
        let (nm, ab) = (s(to, "name"), s(to, "answeredBy"));
        let where_ = if !nm.is_empty() || !ab.is_empty() { if s(to, "type") == "decision" { format!("{ab} is asked \"{}\"", s(to, "label")) } else { nm } } else { let l = s(to, "label"); if l.is_empty() { s(to, "type") } else { l } };
        let mr = n(item, "maxRounds") as i64;
        let rounds = if mr > 0 { format!(" (round {} of {mr})", ((n(item, "used") as i64) + 1).min(mr)) } else { String::new() };
        return d.child(note("hint", format!("Then: {where_}{rounds}"))).into_any_element();
    }
    let id = s(dc, "id");
    let kind = s(dc, "kind");
    let many = kind == "many";
    let cur = fx.e.dry_answers.get(&id).cloned();
    let handles: Vec<String> = if kind == "yesno" { vec!["yes".into(), "no".into()] } else { arr(dc, "options").iter().map(|o| s(o, "id")).collect() };
    let label = |h: &str| -> String { if kind == "yesno" { if h == "yes" { "Yes".into() } else { "No".into() } } else { arr(dc, "options").iter().find(|o| s(o, "id") == h).map(|o| s(o, "label")).unwrap_or_default() } };
    let picked: Vec<String> = if many {
        match &cur { Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(), None if dc["lastPicks"].is_array() => arr(dc, "lastPicks").iter().filter_map(|x| x.as_str().map(String::from)).collect(), _ => vec![] }
    } else { cur.as_ref().and_then(|v| v.as_str()).map(|x| vec![x.to_string()]).unwrap_or_default() };
    let q = { let x = s(dc, "question"); if x.is_empty() { s(dc, "label") } else { x } };
    let mut row = div().flex().flex_wrap().gap(T::SP_6);
    for h in &handles {
        let on = picked.contains(h);
        let (id2, h2) = (id.clone(), h.clone());
        row = row.child(w::toggle(SharedString::from(format!("e-dry-pick-{id}-{h}")), label(h), on).on_click(move |_, _, cx| fact("e-dry-answer", &id2, &h2, cx)));
    }
    if many {
        let id2 = id.clone();
        let none_on = matches!(&cur, Some(Value::Array(a)) if a.is_empty());
        row = row.child(w::toggle(SharedString::from(format!("e-dry-none-{id}")), "None", none_on).on_click(move |_, _, cx| fact("e-dry-answer", &id2, "", cx)));
    }
    d = d.child(div().text_size(T::TEXT_12).child(q)).child(row);
    if many && dc["lastPicks"].is_array() && cur.is_none() { d = d.child(note("hint", "Last time's picks are used unless you change them.")); }
    let chosen: Vec<String> = if many { if picked.is_empty() { vec!["none".into()] } else { picked } } else { picked };
    for h in chosen {
        if let Some(br) = arr(dc, "branches").iter().find(|x| s(x, "answer") == h) {
            if br["next"].is_object() { d = d.child(div().flex().flex_col().gap(T::SP_6).pl(T::SP_10).border_l_2().border_color(T::border()).child(dry_route(&br["next"], fx, window, cx))); }
        }
    }
    d.into_any_element()
}

pub fn dry_props(cx: &mut App) -> Value {
    with(cx, |e| match &e.run {
        None => json!({ "on": false }),
        Some(r) => json!({ "on": true, "status": s(r, "status"), "handoffs": n(r, "handoffs"), "error": e.dry_err, "log": e.dry_log,
            "tokens": arr(r, "tokens").iter().filter(|t| ["work", "answer", "ask", "wait"].contains(&s(t, "status").as_str())).map(|t| json!({ "id": s(t, "id"), "nodeId": s(t, "nodeId"), "status": s(t, "status") })).collect::<Vec<_>>(),
            "edgeCounts": r["edgeCounts"].clone() }),
    })
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

fn pal_item(i: usize, label: &str, icon: IconName, ghost: bool, kind: String, hint: &str) -> Stateful<Div> {
    let k2 = kind.clone();
    let el = div().id(("e-pal", i)).flex().items_center().gap(T::SP_8).px(T::SP_8).h(T::CONTROL_SM + T::SP_2).rounded(T::R_FIELD).border_1().border_color(T::border())
        .when(!ghost, |d| d.bg(T::bg_card())).hover(|d| d.border_color(T::accent())).cursor_grab()
        .text_color(if ghost { T::text_secondary() } else { T::text() })
        .child(w::icon(icon, T::ICON_MD)).child(div().flex_1().min_w_0().truncate().text_size(T::TEXT_12).child(label.to_string()))
        .on_click(move |_, _, cx| fact("e-add", &k2, "", cx))
        .on_drag(PalDrag { kind, label: label.to_string() }, |d, _, _, cx| cx.new(|_| PalGhost(d.label.clone())));
    w::tip(el, hint.to_string())
}

/// The Palette (.ff-palette): the people, someone new, the flow's steps.
fn palette(people: &[Opt]) -> Vec<AnyElement> {
    let mut out = vec![sub("PEOPLE").into_any_element()];
    let mut i = 0;
    for p in people { out.push(pal_item(i, &p.label, IconName::User, false, format!("employee:{}", p.id), "").into_any_element()); i += 1; }
    out.push(pal_item(i, "Someone new", IconName::Plus, true, "employee:".into(), "").into_any_element());
    out.push(sub("FLOW").into_any_element());
    for (label, icon, kind, hint) in [("Yes / no question", IconName::GitBranch, "decision:yesno", ""), ("Pick one", IconName::GitBranch, "decision:one", ""), ("Pick any (parallel)", IconName::GitBranch, "decision:many", "Comes with its Join"),
        ("Join", IconName::GitMerge, "join", ""), ("Ask me", IconName::MessageCircleQuestionMark, "ask", ""), ("Wait", IconName::Clock, "wait", "A time, a command, a signal")] {
        i += 1;
        out.push(pal_item(i, label, icon, false, kind.into(), hint).into_any_element());
    }
    out.push(note("hint", "Drag onto the canvas, or tap to add. Connect an output (bottom) to a step's top. Drag from the side dot of a person to leave a note.").into_any_element());
    out
}

/// The editor.
pub fn render(fx: &mut Fx, window: &mut Window, cx: &mut Context<Fx>) -> AnyElement {
    let (ed, nodes, edges, people, insp) = re(fx, |e| (e.ed.clone(), e.nodes.clone(), e.edges.clone(), e.people.clone(), e.insp.clone()));
    if ed.state == "loading" || ed.state.is_empty() { return div().p(T::SP_24).child(note("note", "Loading…")).into_any_element(); }
    if ed.state == "error" {
        return div().flex().flex_col().p(T::SP_24).gap(T::SP_10).child(note("note", ed.error.clone()))
            .child(div().child(btn("e-back-err", "Back", None, "", true, false).on_click(a0("e-back")))).into_any_element();
    }
    // Narrow: the web's phone. Its first draw starts with no pane open.
    let phone = parts::bounds_of(&fx.p, "fx-ed").map(|b| b.size.width < T::FACTORIES_NARROW).unwrap_or(false);
    fx.e.phone = phone;
    if phone && !fx.e.phone_seen { fx.e.phone_seen = true; fx.e.tab.clear(); fx.e.ed.tab.clear(); }
    let tab_now = fx.e.tab.clone();
    let dry = fx.e.run.is_some();
    let save_c = if ed.save == "conflict" || ed.save == "error" { T::red() } else if ed.draft { T::yellow() } else if ed.save == "saving" || ed.save == "dirty" { T::text_secondary() } else { T::text_muted() };
    let show_status = !phone || ed.save == "conflict" || ed.save == "error";
    let status = div().id("e-status").text_size(T::TEXT_11).text_color(save_c).child(format!("{}{}", ed.save_label, if ed.draft { " · draft" } else { "" }));
    let status = w::tip(status, if ed.draft { "Saved as a draft: fix the problems to run it" } else { "" });
    let top = div().flex().flex_none().items_center().gap(T::SP_6).px(T::SP_8).h(T::CONTROL + T::SP_10).bg(T::bg_secondary()).border_b_1().border_color(T::border())
        .child(w::icon_button("e-back", IconName::ArrowLeft, "Back").on_click(a0("e-back")))
        // .ff-name: the factory's settings.
        .child(w::ghost("e-name", if ed.name.is_empty() { "Untitled factory".to_string() } else { ed.name.clone() }).tooltip("Factory settings").on_click(a0("e-settings")))
        .when(show_status, |d| d.child(status))
        .when(ed.save == "conflict", |d| d.child(btn("e-reload", "Reload", None, "", true, false).on_click(a0("e-reload"))))
        .when(ed.save == "error", |d| d.child(btn("e-retry", "Retry", None, "", true, false).on_click(a0("e-save"))))
        .child(div().flex_1())
        .child(w::disabled(w::icon_button("e-undo", IconName::Undo2, "Undo (Ctrl+Z)"), !ed.can_undo || dry).on_click(a0("e-undo")))
        .child(w::disabled(w::icon_button("e-redo", IconName::Redo2, "Redo (Ctrl+Shift+Z)"), !ed.can_redo || dry).on_click(a0("e-redo")))
        .child(w::disabled(w::icon_button("e-tidy", IconName::LayoutGrid, "Tidy the layout"), dry).on_click(a0("e-tidy")))
        .child(w::icon_button("e-fit", IconName::Maximize, "Fit to screen").on_click(|_, _, cx| fact("cv-fit", "e", "", cx)));
    let mut row = div().flex().flex_1().min_h_0();
    if !phone {
        row = row.child(div().id("e-palette").flex().flex_col().flex_none().w(T::FACTORIES_LEFT).bg(T::bg_secondary()).border_r_1().border_color(T::border()).overflow_y_scroll().p(T::SP_12).gap(T::SP_10)
            .children(palette(&people)));
    }
    let cv = parts::canvas_view(&mut fx.p, cx.entity(), parts::Canvas { which: "e", nodes, edges, zoom: ed.zoom, pan_x: ed.pan_x, pan_y: ed.pan_y, editable: !dry, minimap: !phone, controls_top: phone, bounds: (ed.bx, ed.by, ed.bw, ed.bh), conn: ed.conn.then(|| ed.conn_path.clone()), cb: calls() }, window, cx);
    let mut canvas_box = div().relative().flex().flex_1().min_w_0()
        .on_drop(|d: &PalDrag, window, cx| {
            let (x, y) = { let o = parts::bounds_in(cx, "cv-e").map(|b| b.origin).unwrap_or_default(); let m = window.mouse_position(); (f32::from(m.x - o.x), f32::from(m.y - o.y)) };
            fact("e-add-at", &d.kind, &format!("{x},{y}"), cx);
        })
        .child(cv);
    // .ff-tabs: (Add on a phone,) Edit, Team, Issues (Fix n), Dry run.
    let mut ids: Vec<(&'static str, IconName, String)> = vec![];
    if phone { ids.push(("add", IconName::Plus, "Add".into())); }
    ids.push(("edit", IconName::SlidersHorizontal, "Edit".into()));
    ids.push(("team", IconName::Users, "Team".into()));
    ids.push(("issues", IconName::TriangleAlert, ed.issues_label.clone()));
    ids.push(("dry", IconName::Route, "Dry run".into()));
    let on = ids.iter().position(|t| t.0 == tab_now).or_else(|| ids.iter().position(|t| t.0 == "edit")).unwrap_or(0);
    let marked: Vec<bool> = ids.iter().map(|t| t.0 == "issues" && ed.issues_bad).collect();
    let keys: Vec<&'static str> = ids.iter().map(|t| t.0).collect();
    let segs = |labels: bool| ids.iter().enumerate().map(|(i, t)| w::SegTab { icon: Some(t.1), label: if labels || i == on { t.2.clone().into() } else { SharedString::default() }, ..Default::default() }).collect::<Vec<_>>();
    let pick = { let keys = keys.clone(); move |i: usize, _: &mut Window, cx: &mut App| fact("e-tab", keys.get(i).copied().unwrap_or("edit"), "", cx) };
    let tabs = div().flex().flex_none().items_center().gap(T::SP_6).p(T::SP_6).border_b_1().border_color(T::border())
        .child(div().flex_1().min_w_0().child(w::seg_tabs_marked("e-tab", segs(!phone), on, &marked, pick.clone())))
        .when(phone, |d| d.child(w::icon_button("e-sheet-x", IconName::X, "Close").on_click(|_, _, cx| fact("e-tab", "", "", cx))));
    let pane = div().id("e-pane").flex().flex_col().flex_1().min_h_0().overflow_y_scroll().p(T::SP_12).gap(T::SP_10);
    let pane = if tab_now == "add" { pane.children(palette(&people)) } else { match insp.as_ref() {
        Some(d) => match d.mode.as_str() {
            "node" => node_insp(d, pane, fx, window, cx),
            "edge" => edge_insp(d, pane, fx, window, cx),
            "team" => team_insp(d, pane),
            "person" => person_insp(d, pane, fx, window, cx),
            "issues" => issues_insp(d, pane),
            "dry" => dry_insp(d, pane, fx, window, cx),
            _ => settings_insp(d, pane, fx, window, cx),
        },
        None => pane,
    } };
    let right = div().flex().flex_col().bg(T::bg_secondary()).child(tabs).child(pane);
    if !phone {
        row = row.child(canvas_box).child(right.flex_none().w(T::FACTORIES_RIGHT).border_l_1().border_color(T::border()));
    } else {
        // The pane as a sheet over the canvas, or the bar of tabs under it.
        if !tab_now.is_empty() {
            canvas_box = canvas_box.child(right.absolute().left_0().right_0().bottom_0().max_h(relative(T::FACTORIES_CANVAS_SHARE)).border_t_1().border_color(T::border()).rounded_t(T::R_CARD).shadow_lg()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).on_scroll(|_, _, cx| cx.stop_propagation()));
        }
        row = row.child(canvas_box);
    }
    let bar = (phone && tab_now.is_empty()).then(|| div().flex().flex_none().p(T::SP_4).bg(T::bg_secondary()).border_t_1().border_color(T::border())
        .child(div().flex_1().child(w::seg_tabs_marked("e-bar", segs(true), usize::MAX, &marked, pick))));
    div().relative().flex().flex_col().size_full().min_h_0()
        .child(parts::probe(cx.entity(), "fx-ed", |_, _| {}))
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
        .child(top).child(row).children(bar).into_any_element()
}
