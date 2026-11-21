//! Factories: the run engine the web's Dry run walks (src/factory-engine.js),
//! pure, on the run as JSON. Every entry point works on a copy of the run and
//! returns `{ ok: true, run, effects }` or `{ ok: false, error }` with the
//! web's words; `requirements` returns what the person at a token must answer.
//!
//! Tokens live in an arena while a call runs, so a token the JS has dropped
//! from `run.tokens` but still holds (and changes) behaves the same here.

use super::model::{arr, truthy};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

const MANAGER_ID: &str = "manager";
const NO_PROGRESS_REPEATS: f64 = 3.0;
const MANAGER_REOPEN_LIMIT: f64 = 2.0;
const HISTORY_MAX: usize = 2000;
const TEXT_MAX: usize = 20_000;
/// What the JS throws where it reads a property of a missing node or group.
const UNDEF_ID: &str = "Cannot read properties of undefined (reading 'id')";

// ─── JS values ───

fn tr(v: Option<&Value>) -> bool { v.map(truthy).unwrap_or(false) }
/// `a || b`.
fn or<'a>(a: Option<&'a Value>, b: Option<&'a Value>) -> Option<&'a Value> { if tr(a) { a } else { b } }
/// `v || ''`.
fn or_str(v: Option<&Value>) -> Value { if tr(v) { v.cloned().unwrap_or_default() } else { json!("") } }
fn some<T: Into<Value>>(x: T) -> Option<Value> { Some(x.into()) }
fn opt(x: Option<&str>) -> Option<Value> { x.map(Value::from) }

/// String(n).
fn js_num(x: f64) -> String {
    if x.is_nan() { return "NaN".into(); }
    if x.is_infinite() { return if x > 0.0 { "Infinity" } else { "-Infinity" }.into(); }
    if x == 0.0 { return "0".into(); }
    if x.fract() == 0.0 && x.abs() < 1e21 { return format!("{x:.0}"); }
    format!("{x}")
}
/// String(v).
fn js_string(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.as_f64().map(js_num).unwrap_or_else(|| n.to_string()),
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(|x| if x.is_null() { String::new() } else { js_string(x) }).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}
/// String(v) where v may be undefined.
fn js_string_opt(v: Option<&Value>) -> String { v.map(js_string).unwrap_or_else(|| "undefined".into()) }
/// s.slice(0, n), counted in UTF-16 units.
fn slice16(s: &str, n: usize) -> String {
    let mut used = 0;
    let mut out = String::new();
    for c in s.chars() {
        used += c.len_utf16();
        if used > n { break; }
        out.push(c);
    }
    out
}
/// s.trim().
fn js_trim(s: &str) -> String {
    s.trim_matches(|c: char| matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')).to_string()
}
/// text(): a string, at most TEXT_MAX long; '' for null.
fn text(v: Option<&Value>) -> String {
    match v { None | Some(Value::Null) => String::new(), Some(x) => slice16(&js_string(x), TEXT_MAX) }
}
fn st(v: &Value, k: &str) -> String { v.get(k).and_then(Value::as_str).unwrap_or("").to_string() }
fn num(v: Option<&Value>) -> f64 { v.and_then(Value::as_f64).unwrap_or(0.0) }
fn jn(x: f64) -> Value { if x.fract() == 0.0 && x.abs() < 9.0e15 { json!(x as i64) } else { json!(x) } }
fn set(v: &mut Value, k: &str, x: Value) {
    if !v.is_object() { *v = Value::Object(Map::new()); }
    if let Value::Object(m) = v { m.insert(k.to_string(), x); }
}
fn field<'a>(v: &'a mut Value, k: &str) -> &'a mut Value {
    if !v.is_object() { *v = Value::Object(Map::new()); }
    &mut v[k]
}
fn del(v: &mut Value, k: &str) { if let Value::Object(m) = v { m.shift_remove(k); } }
/// `o[k] = x`, where undefined drops the key from the JSON.
fn put(v: &mut Value, k: &str, x: Option<Value>) { match x { Some(x) => set(v, k, x), None => del(v, k) } }
/// An object literal whose undefined members are left out.
fn obj(pairs: Vec<(&str, Option<Value>)>) -> Value {
    Value::Object(pairs.into_iter().filter_map(|(k, v)| v.map(|v| (k.to_string(), v))).collect())
}
fn fail(msg: &str) -> Value { json!({ "ok": false, "error": msg }) }

/// Object.keys() order: array-index keys ascending first, then the rest.
fn js_keys<'a>(keys: impl Iterator<Item = &'a String>) -> Vec<String> {
    let is_index = |k: &str| k == "0" || (!k.starts_with('0') && !k.is_empty() && k.len() <= 10 && k.bytes().all(|b| b.is_ascii_digit()) && k.parse::<u64>().is_ok_and(|n| n < 4_294_967_295));
    let (mut ints, rest): (Vec<String>, Vec<String>) = keys.cloned().partition(|k| is_index(k));
    ints.sort_by_key(|k| k.parse::<u64>().unwrap_or(0));
    ints.extend(rest);
    ints
}

/// stable(): JSON.stringify with every object's entries sorted.
fn stable(v: &Value) -> String {
    match v {
        Value::Array(a) => format!("[{}]", a.iter().map(stable).collect::<Vec<_>>().join(",")),
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort_by_cached_key(|k| js_string(&json!([k, m[k.as_str()]])).encode_utf16().collect::<Vec<u16>>());
            let parts: Vec<String> = js_keys(keys.into_iter()).iter().map(|k| format!("{}:{}", Value::String(k.clone()), stable(&m[k.as_str()]))).collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Number(n) => match n.as_f64() { Some(x) if x.is_finite() => js_num(x), _ => "null".into() },
        x => x.to_string(),
    }
}

// ─── The factory, indexed ───

struct Ix {
    nodes: HashMap<String, Value>,
    roster: HashMap<String, Value>,
    out_edge: HashMap<String, Value>,
    notes_from: HashMap<String, Vec<Value>>,
    manager: Option<Value>,
    join_of: HashMap<String, Value>,
}

fn index(f: &Value) -> Ix {
    let mut out_edge = HashMap::new();
    let mut notes_from: HashMap<String, Vec<Value>> = HashMap::new();
    for e in arr(f, "edges") {
        if st(e, "kind") == "note" { notes_from.entry(st(e, "source")).or_default().push(e.clone()); }
        else { out_edge.insert(format!("{}|{}", st(e, "source"), st(e, "sourceHandle")), e.clone()); }
    }
    Ix {
        nodes: arr(f, "nodes").iter().map(|n| (st(n, "id"), n.clone())).collect(),
        roster: arr(f, "roster").iter().map(|r| (st(r, "id"), r.clone())).collect(),
        out_edge,
        notes_from,
        manager: arr(f, "nodes").iter().find(|n| st(n, "type") == "manager").cloned(),
        join_of: arr(f, "nodes").iter().filter(|n| st(n, "type") == "join").map(|n| (st(&n["join"], "forkId"), n.clone())).collect(),
    }
}

impl Ix {
    fn node(&self, id: &str) -> Option<Value> { self.nodes.get(id).cloned() }
    fn out(&self, src: &str, h: &str) -> Option<Value> { self.out_edge.get(&format!("{src}|{h}")).cloned() }
    /// nameOf().
    fn name_of(&self, id: Option<&str>) -> String {
        if let Some(n) = id.and_then(|i| self.roster.get(i)).and_then(|r| r.get("name")).filter(|x| truthy(x)) { return js_string(n); }
        match id { Some(MANAGER_ID) => "Manager".into(), Some(i) if !i.is_empty() => i.into(), _ => "someone".into() }
    }
    /// nodeLabel().
    fn label(&self, node: Option<&Value>) -> String {
        let Some(node) = node else { return "?".into() };
        if let Some(l) = node.get("label").filter(|x| truthy(x)) { return js_string(l); }
        let t = st(node, "type");
        if t == "employee" || t == "manager" { return self.name_of(Some(&roster_of(node))); }
        t
    }
}

/// rosterOf(): the roster id of the person at an agent node.
fn roster_of(node: &Value) -> String {
    match st(node, "type").as_str() { "manager" => MANAGER_ID.into(), "employee" => st(node, "employeeId"), _ => String::new() }
}

fn answered_by(node: &Value) -> Option<String> { node.get("decision").and_then(|d| d.get("answeredBy")).filter(|x| truthy(x)).map(js_string) }

/// handleLabel() (the engine's own words: "none of these").
fn handle_label(node: &Value, h: &str) -> String {
    let find = |list: &[Value]| list.iter().find(|o| o.get("id").and_then(Value::as_str) == Some(h)).and_then(|o| o.get("label")).filter(|x| truthy(x)).map(js_string).unwrap_or_else(|| h.to_string());
    match st(node, "type").as_str() {
        "decision" => if h == "none" { "none of these".into() } else if h == "yes" || h == "no" { h.into() } else { find(arr(&node["decision"], "options")) },
        "ask" => find(arr(&node["ask"], "replies")),
        _ => h.into(),
    }
}

// ─── Create, start ───

/// createRun(): a new run with its token at Start (the factory snapshotted).
pub(crate) fn create_run(id: &str, project: &str, factory: &Value, task: &str) -> Value {
    let f = super::valid::normalize(factory);
    let start = arr(&f, "nodes").iter().find(|n| st(n, "type") == "start").and_then(|n| n.get("id").cloned()).unwrap_or(json!(""));
    let now = "";
    json!({
        "id": id,
        "project": project,
        "task": slice16(task, TEXT_MAX),
        "snapshot": f,
        "status": "starting",
        "createdAt": now,
        "updatedAt": now,
        "seq": 0,
        "tokens": [{ "id": "t0", "nodeId": start, "ws": "main", "path": [], "status": "moving" }],
        "groups": {},
        "workspaces": { "main": { "id": "main", "parent": null, "name": "main" } },
        "edgeCounts": {},
        "exhausted": {},
        "lastPicks": {},
        "inbox": {},
        "handoffs": 0,
        "fingerprints": {},
        "history": [],
        "outcome": null,
        "ledger": { "seq": 0, "items": [] },
    })
}

/// start(): the token leaves Start; the Manager gets the task.
pub(crate) fn start(run: &Value) -> Value {
    let mut c = Run::load(run);
    let Some(&t) = c.live.first() else { return fail("This factory has no way from Start to the Manager.") };
    let Some(e) = c.ix.out(&st(&c.toks[t], "nodeId"), "out") else { return fail("This factory has no way from Start to the Manager.") };
    c.traverse(t, &e, None, "");
    if !c.errors.is_empty() { return fail(&c.errors.join("\n")); }
    set(&mut c.r, "status", json!("running"));
    c.log(json!({ "type": "start" }));
    let effects = std::mem::take(&mut c.effects);
    json!({ "ok": true, "run": c.into_run(), "effects": effects })
}

// ─── Requirements ───

/// requirements(): what the token's person must provide, as a tree (null when nothing).
pub(crate) fn requirements(run: &Value, token_id: &str) -> Value {
    let ix = index(&run["snapshot"]);
    let Some(t) = arr(run, "tokens").iter().find(|x| x.get("id").and_then(Value::as_str) == Some(token_id)) else { return Value::Null };
    if tr(t.get("delegate")) {
        let d = &t["delegate"];
        return obj(vec![("mode", some("delegate")), ("who", d.get("employeeId").cloned()), ("canFinish", some(false)), ("route", Some(Value::Null)), ("noteTargets", some(json!([]))), ("task", d.get("task").cloned())]);
    }
    let node = ix.node(&st(t, "nodeId"));
    match st(t, "status").as_str() {
        "work" => {
            let Some(node) = node else { return Value::Null };
            let who = roster_of(&node);
            let nid = st(&node, "id");
            let route = ix.out(&nid, "out").map(|out| describe_edge(run, &ix, &out, Some(who.as_str()), &HashSet::new())).unwrap_or(Value::Null);
            let notes: Vec<Value> = ix.notes_from.get(&nid).map(|l| l.iter().map(|e| {
                let to = ix.nodes.get(&st(e, "target")).map(roster_of).unwrap_or_default();
                obj(vec![("key", e.get("target").cloned()), ("edgeId", e.get("id").cloned()), ("label", Some(or_str(e.get("noteLabel")))), ("to", some(ix.name_of(Some(&to))))])
            }).collect()).unwrap_or_default();
            json!({ "mode": "work", "who": who, "canFinish": st(&node, "type") == "manager", "route": route, "noteTargets": notes })
        }
        "answer" => {
            let Some(node) = node else { return Value::Null };
            let route = describe_decision(run, &ix, &node, t.get("answerer").and_then(Value::as_str), &HashSet::new());
            obj(vec![("mode", some("answer")), ("who", t.get("answerer").cloned()), ("canFinish", some(false)), ("route", Some(route)), ("noteTargets", some(json!([])))])
        }
        _ => Value::Null,
    }
}

fn describe_edge(run: &Value, ix: &Ix, e: &Value, who: Option<&str>, seen: &HashSet<String>) -> Value {
    let eid = st(e, "id");
    let target = ix.node(&st(e, "target"));
    let used = run.get("edgeCounts").and_then(|c| c.get(&eid)).filter(|x| truthy(x)).cloned().unwrap_or(json!(0));
    let mut item = obj(vec![("edgeId", e.get("id").cloned()), ("requireNote", e.get("requireNote").cloned()), ("noteLabel", Some(or_str(e.get("noteLabel")))), ("maxRounds", e.get("maxRounds").cloned()), ("used", Some(used))]);
    // "None of these" straight into its Join passes through: say where it goes on to.
    if let Some(tg) = &target
        && st(tg, "type") == "join" && st(e, "sourceHandle") == "none"
        && let Some(on) = ix.out(&st(tg, "id"), "out")
    {
        let mut x = describe_edge(run, ix, &on, None, seen);
        put(&mut x, "requireNote", or(e.get("requireNote"), on.get("requireNote")).cloned());
        return x;
    }
    match &target {
        Some(tg) if st(tg, "type") == "decision" && answered_by(tg).as_deref().or(who) == who && !seen.contains(&st(tg, "id")) => {
            let d = describe_decision(run, ix, tg, who, seen);
            set(&mut item, "decision", d["decision"].clone());
        }
        _ => set(&mut item, "to", describe_target(ix, target.as_ref(), who)),
    }
    item
}

fn describe_decision(run: &Value, ix: &Ix, node: &Value, who: Option<&str>, seen: &HashSet<String>) -> Value {
    let nid = st(node, "id");
    let mut s = seen.clone();
    s.insert(nid.clone());
    let d = node.get("decision").cloned().unwrap_or_default();
    let kind = st(&d, "kind");
    let opts = arr(&d, "options");
    let mut handles: Vec<String> = if kind == "yesno" { vec!["yes".into(), "no".into()] } else { opts.iter().map(|o| st(o, "id")).collect() };
    if kind == "many" { handles.push("none".into()); }
    let options: Vec<Value> = if kind == "yesno" { vec![] } else { opts.iter().map(|o| obj(vec![("id", o.get("id").cloned()), ("label", o.get("label").cloned())])).collect() };
    let last = if kind == "many" { run.get("lastPicks").and_then(|l| l.get(&nid)).filter(|x| truthy(x)).cloned().unwrap_or_default() } else { Value::Null };
    let branches: Vec<Value> = handles.iter().map(|h| json!({
        "answer": h,
        "label": handle_label(node, h),
        "next": ix.out(&nid, h).map(|e| describe_edge(run, ix, &e, who, &s)).unwrap_or_default(),
    })).collect();
    json!({ "decision": obj(vec![
        ("id", node.get("id").cloned()),
        ("kind", d.get("kind").cloned()),
        ("question", d.get("question").cloned()),
        ("label", node.get("label").cloned()),
        ("options", some(options)),
        ("lastPicks", Some(last)),
        ("review", some(tr(d.get("review")))),
        ("branches", some(branches)),
    ]) })
}

fn describe_target(ix: &Ix, node: Option<&Value>, who: Option<&str>) -> Value {
    let Some(node) = node else { return json!({ "type": "missing" }) };
    let t = st(node, "type");
    let base = vec![("type", node.get("type").cloned()), ("nodeId", node.get("id").cloned()), ("label", some(ix.label(Some(node))))];
    let extra = if t == "decision" {
        let by = answered_by(node);
        Some(("answeredBy", some(ix.name_of(by.as_deref().or(who)))))
    } else if t == "employee" || t == "manager" {
        Some(("name", some(ix.name_of(Some(&roster_of(node))))))
    } else { None };
    obj(base.into_iter().chain(extra).collect())
}

// ─── Handoff, replies, waits ───

/// handoff(): a person has finished their step (`tree_hash` "" for none).
pub(crate) fn handoff(run: &Value, token_id: &str, payload: &Value, tree_hash: &str) -> Value { handoff_capped(run, token_id, payload, tree_hash, None) }

/// handoff() with meta.caps.maxHandoffs (the journal's replay).
pub(crate) fn handoff_capped(run: &Value, token_id: &str, payload: &Value, tree_hash: &str, cap: Option<f64>) -> Value {
    let mut c = Run::load(run);
    c.cap = cap;
    let t = match c.find(token_id) { Some(t) if matches!(c.status(t).as_str(), "work" | "answer") => t, _ => return fail("There is nothing waiting on you right now.") };
    let p = if payload.is_object() || payload.is_array() { payload.clone() } else { json!({}) };
    if tr(c.toks[t].get("delegate")) { return c.finish_delegate(t, &p); }
    let node = c.ix.node(&st(&c.toks[t], "nodeId"));
    let work = c.status(t) == "work";
    let who: Option<String> = if work { Some(node.as_ref().map(roster_of).unwrap_or_default()) } else { c.toks[t].get("answerer").and_then(Value::as_str).map(String::from) };
    let who = who.as_deref();
    // The Manager deciding, in either mode: it may reopen a used-up loop and
    // cannot escalate to itself. Only a working Manager can finish.
    let is_manager = who == Some(MANAGER_ID);
    let summary = js_trim(&text(p.get("summary")));
    if summary.is_empty() && !tr(p.get("escalate")) { return fail("Write a short summary of what you did (summary)."); }
    if p.get("answers").is_some_and(|a| !a.is_null() && !a.is_object()) { return fail("answers must be an object keyed by decision id."); }
    if p.get("notes").is_some_and(|a| !a.is_null() && !a.is_object()) { return fail("notes must be an object keyed by id."); }
    let Some(node) = node else { return fail(UNDEF_ID) };
    let nid = st(&node, "id");
    let (from_name, from_label) = (c.ix.name_of(who), c.ix.label(Some(&node)));
    c.from = Some((from_name.clone(), from_label.clone()));
    c.summary = summary.clone();
    c.payload = p.clone();
    c.is_manager = is_manager;

    // Escalation: straight to the Manager, with the reason.
    if tr(p.get("escalate")) {
        if is_manager { return fail("You are the Manager; there is nobody to escalate to. Finish, or talk to the person in this chat."); }
        c.inc_handoffs();
        let reason = text(p.get("escalate"));
        c.log(obj(vec![("type", some("escalate")), ("nodeId", some(nid.as_str())), ("by", opt(who)), ("reason", some(reason.as_str()))]));
        c.escalate_token(t, &format!("{from_name} escalated: {reason}"));
        return c.finish();
    }

    // Unknown note keys are a mistake worth pointing out.
    let mut allowed: HashSet<String> = arr(&c.r["snapshot"], "edges").iter().filter(|e| tr(e.get("requireNote"))).map(|e| st(e, "id")).collect();
    if work {
        for e in c.ix.notes_from.get(&nid).cloned().unwrap_or_default() { allowed.insert(st(&e, "id")); allowed.insert(st(&e, "target")); }
    }
    if let Some(Value::Object(m)) = p.get("notes") {
        for k in js_keys(m.keys()) {
            if !allowed.contains(&k) { return fail(&format!("notes.{k} is not a note you can leave here. Use the keys listed in your instructions.")); }
        }
    }

    // The review ledger: findings filed, fixed, verified, reopened.
    let lerr = apply_ledger(&mut c.r, who, &p);
    if !lerr.is_empty() { return fail(&lerr); }

    c.inc_handoffs();
    let answers = p.get("answers").filter(|x| truthy(x)).cloned().unwrap_or(json!({}));

    // No progress: same step, same answers, same tree, again and again.
    if work && !is_manager && !tree_hash.is_empty() {
        let fp = format!("{nid}|{}|{tree_hash}", stable(&answers));
        let n = num(c.r.get("fingerprints").and_then(|f| f.get(&fp))) + 1.0;
        set(field(&mut c.r, "fingerprints"), &fp, jn(n));
        if n >= NO_PROGRESS_REPEATS {
            c.log(obj(vec![("type", some("no-progress")), ("nodeId", some(nid.as_str())), ("by", opt(who))]));
            c.deliver_notes(&node);
            let ls = ledger_summary(&mut c.r);
            c.escalate_token(t, &format!("No progress: \"{from_label}\" has handed off {} times with the same answers and no change to the code.{ls}", js_num(n)));
            return c.finish();
        }
    }

    let mode = c.status(t);
    c.log(obj(vec![("type", some("handoff")), ("nodeId", some(nid.as_str())), ("by", opt(who)), ("mode", some(mode)), ("summary", some(summary.as_str())), ("answers", Some(answers)), ("finish", some(tr(p.get("finish"))))]));

    if is_manager && work && tr(p.get("finish")) {
        set(&mut c.toks[t], "status", json!("finished"));
        set(&mut c.r, "status", json!("finishing"));
        c.keep_status = true;
        set(&mut c.r, "outcome", json!({ "summary": summary }));
        let tid = c.toks[t].get("id").cloned();
        c.effects.push(obj(vec![("type", some("finish")), ("tokenId", tid), ("summary", some(summary.as_str()))]));
        return c.finish();
    }

    if work {
        c.deliver_notes(&node);
        let Some(out) = c.ix.out(&nid, "out") else {
            return fail(if is_manager { "There is no step after you in this factory. Finish (finish: true) when the work is done." } else { "This step has nowhere to go." });
        };
        c.traverse(t, &out, who, "");
    } else {
        c.eval_decision(t, &node, who);
    }
    if !c.errors.is_empty() { return fail(&c.errors.join("\n")); }
    c.cap_check();
    c.finish()
}

/// escalate(): hand a token to the Manager with a reason (a stall, a merge
/// conflict, a crash the runner could not recover from).
pub(crate) fn escalate(run: &Value, token_id: &str, reason: &str) -> Value {
    let mut c = Run::load(run);
    let Some(t) = c.find(token_id) else { return fail("No such token.") };
    let reason = text(Some(&json!(reason)));
    if tr(c.toks[t].get("delegate")) { return c.finish_delegate(t, &json!({ "escalate": reason })); }
    let nid = c.toks[t].get("nodeId").cloned();
    c.log(obj(vec![("type", some("escalate")), ("nodeId", nid), ("reason", some(reason.as_str()))]));
    c.escalate_token(t, &reason);
    c.finish()
}

/// reopen(): take a finished run back to work; the Manager gets the token
/// again with the reason.
pub(crate) fn reopen(run: &Value, reason: &str, as_message: bool) -> Value {
    let mut c = Run::load(run);
    let Some(t) = c.live.iter().copied().find(|&i| st(&c.toks[i], "status") == "finished") else { return fail("The run has not finished, so there is nothing to reopen.") };
    let Some(mgr) = c.ix.manager.clone() else { return fail(UNDEF_ID) };
    let reason = text(Some(&json!(reason)));
    c.from = Some(("The person".into(), String::new()));
    c.log(json!({ "type": "reopen", "reason": reason }));
    set(&mut c.r, "outcome", Value::Null);
    set(&mut c.toks[t], "nodeId", mgr.get("id").cloned().unwrap_or_default());
    set(&mut c.toks[t], "status", json!("work"));
    if as_message { c.activate(t, &mgr, "work", &reason, ""); } else { c.activate(t, &mgr, "work", "", &reason); }
    c.finish()
}

/// delegate(): the Manager hands a one-off job to someone on the team; its
/// own token waits until the job comes back.
pub(crate) fn delegate(run: &Value, token_id: &str, employee_id: &str, task: &str, cap: Option<f64>) -> Value {
    let mut c = Run::load(run);
    c.cap = cap;
    let t = match c.find(token_id) { Some(t) if matches!(c.status(t).as_str(), "work" | "answer") && !tr(c.toks[t].get("delegate")) => t, _ => return fail("You can only hand out a job while it is your turn.") };
    let person = if tr(c.toks[t].get("delegate")) { st(&c.toks[t]["delegate"], "employeeId") }
        else if c.status(t) == "answer" { st(&c.toks[t], "answerer") }
        else { c.ix.node(&st(&c.toks[t], "nodeId")).as_ref().map(roster_of).unwrap_or_default() };
    if person != MANAGER_ID { return fail("Only the Manager hands out jobs."); }
    let want = employee_id.trim().to_lowercase();
    let emp = c.ix.roster.get(employee_id).cloned().or_else(|| c.ix.roster.values().find(|x| st(x, "name").to_lowercase() == want).cloned());
    let Some(emp) = emp.filter(|e| st(e, "id") != MANAGER_ID) else {
        let team: Vec<String> = c.ix.roster.values().filter(|x| st(x, "id") != MANAGER_ID).map(|x| st(x, "name")).collect();
        return fail(&format!("Nobody called \"{employee_id}\" is on your team. The team: {}.", team.join(", ")));
    };
    let job = js_trim(&text(Some(&json!(task))));
    if job.is_empty() { return fail("Say what the job is."); }
    let tid = c.toks[t].get("id").cloned();
    if c.live.iter().any(|&i| c.toks[i].get("delegatedBy") == tid.as_ref()) { return fail("You are already waiting on a job you handed out."); }
    c.inc_handoffs();
    let mgr_name = c.ix.name_of(Some(MANAGER_ID));
    c.from = Some((mgr_name.clone(), "Manager".into()));
    let prev = c.toks[t].get("status").cloned().unwrap_or_default();
    set(&mut c.toks[t], "prevStatus", prev);
    set(&mut c.toks[t], "status", json!("delegated"));
    let id = c.new_id("t");
    let eid = st(&emp, "id");
    let ws = c.toks[t].get("ws").cloned().unwrap_or_default();
    c.push_token(json!({ "id": id, "nodeId": "", "ws": ws, "path": [], "status": "work", "delegate": { "employeeId": eid, "task": job }, "delegatedBy": tid }));
    c.log(json!({ "type": "delegate", "employeeId": eid, "task": job }));
    c.effects.push(json!({ "type": "activate", "tokenId": id, "nodeId": "", "employeeId": eid, "mode": "delegate", "ws": ws,
        "context": { "from": { "name": mgr_name, "label": "Manager" }, "summary": "", "message": job, "notes": [], "escalation": "" } }));
    c.cap_check();
    c.finish()
}

/// askReply(): the human answered an Ask me node.
pub(crate) fn ask_reply(run: &Value, token_id: &str, reply_id: &str, note: &str) -> Value {
    let mut c = Run::load(run);
    let t = match c.find(token_id) { Some(t) if c.status(t) == "ask" => t, _ => return fail("That question is no longer waiting.") };
    let Some(node) = c.ix.node(&st(&c.toks[t], "nodeId")) else { return fail(UNDEF_ID) };
    let nid = st(&node, "id");
    let Some(e) = c.ix.out(&nid, reply_id) else { return fail("That is not one of the replies.") };
    let note = slice16(note, TEXT_MAX);
    c.from = Some(("You".into(), c.ix.label(Some(&node))));
    c.summary = note.clone();
    c.default_note = note.clone();
    c.log(json!({ "type": "ask-reply", "nodeId": nid, "reply": reply_id, "note": note }));
    c.traverse(t, &e, None, "");
    if !c.errors.is_empty() { return fail(&c.errors.join("\n")); }
    c.finish()
}

/// waitDone(): a Wait node is over, 'done' or 'timeout'.
pub(crate) fn wait_done(run: &Value, token_id: &str, outcome: &str, detail: &str) -> Value {
    let mut c = Run::load(run);
    let t = match c.find(token_id) { Some(t) if c.status(t) == "wait" => t, _ => return fail("That wait is no longer waiting.") };
    let Some(node) = c.ix.node(&st(&c.toks[t], "nodeId")) else { return fail("Cannot read properties of undefined (reading 'wait')") };
    let nid = st(&node, "id");
    let kind = st(&node["wait"], "kind");
    let single = !(kind == "command" || kind == "signal");
    let handle = if single { "out" } else if outcome == "timeout" { "timeout" } else { "done" };
    let Some(e) = c.ix.out(&nid, handle) else { return fail("That wait leads nowhere.") };
    let held = c.toks[t].get("held").filter(|x| truthy(x)).cloned().unwrap_or(json!({}));
    del(&mut c.toks[t], "held");
    let label = c.ix.label(Some(&node));
    let what = js_trim(&slice16(detail, TEXT_MAX));
    let line = format!("{label}: {}{}.", if handle == "timeout" { "gave up waiting" } else { "the wait is over" }, if what.is_empty() { String::new() } else { format!(" ({what})") });
    let msg = match held.get("message").filter(|x| truthy(x)) { Some(m) => format!("{line}\n\n{}", js_string(m)), None => line };
    c.from = Some(match held.get("from").filter(|x| truthy(x)) {
        Some(f) => (js_string_opt(f.get("name")), js_string_opt(f.get("label"))),
        None => (label.clone(), label.clone()),
    });
    c.summary = held.get("summary").filter(|x| truthy(x)).map(js_string).unwrap_or_default();
    c.default_note = msg.clone();
    c.log(json!({ "type": "wait-done", "nodeId": nid, "outcome": if handle == "timeout" { "timeout" } else { "done" }, "detail": what }));
    set(&mut c.toks[t], "status", json!("moving"));
    c.traverse(t, &e, None, &msg);
    if !c.errors.is_empty() { return fail(&c.errors.join("\n")); }
    c.finish()
}

// ─── The review ledger ───

fn ledger_of(r: &mut Value) -> &mut Value {
    if !tr(r.get("ledger")) { set(r, "ledger", json!({ "seq": 0, "items": [] })); }
    field(r, "ledger")
}

fn items_mut(l: &mut Value) -> &mut Vec<Value> {
    let i = field(l, "items");
    if !i.is_array() { *i = Value::Array(vec![]); }
    i.as_array_mut().expect("the ledger's items are an array")
}

fn by_is(i: &Value, who: Option<&str>) -> bool {
    match who { Some(w) => i.get("by").and_then(Value::as_str) == Some(w), None => i.get("by").is_none() }
}

/// applyLedger(): '' or what is wrong.
fn apply_ledger(r: &mut Value, who: Option<&str>, p: &Value) -> String {
    let at = r.get("handoffs").cloned();
    let list = |x: Option<&Value>| -> Vec<Value> {
        match x { Some(Value::Array(a)) => a.clone(), None | Some(Value::Null) => vec![], Some(Value::String(s)) if s.is_empty() => vec![], Some(x) => vec![x.clone()] }
    };
    let ids = |x: Option<&Value>| -> Vec<String> { list(x).iter().map(|v| js_trim(&js_string(v)).to_uppercase()).filter(|s| !s.is_empty()).collect() };
    let (done, verified, reopen, dismiss) = (ids(p.get("done")), ids(p.get("verified")), ids(p.get("reopen")), ids(p.get("dismiss")));
    let l = ledger_of(r);
    let mut seq = num(l.get("seq"));
    let items = items_mut(l);
    let mut by_id: HashMap<String, usize> = HashMap::new();
    for (k, i) in items.iter().enumerate() { if let Some(id) = i.get("id").and_then(Value::as_str) { by_id.insert(id.to_string(), k); } }
    let open_list = |items: &[Value]| {
        let j = items.iter().filter(|i| matches!(st(i, "status").as_str(), "open" | "claimed")).map(|i| match i.get("id") { None | Some(Value::Null) => String::new(), Some(x) => js_string(x) }).collect::<Vec<_>>().join(", ");
        if j.is_empty() { "none".to_string() } else { j }
    };
    for id in done.iter().chain(&verified).chain(&reopen).chain(&dismiss) {
        if !by_id.contains_key(id) { return format!("There is no finding {id}. Findings still open: {}.", open_list(items)); }
    }
    if !dismiss.is_empty() && who != Some(MANAGER_ID) { return "Only the Manager can dismiss a finding.".into(); }
    let earlier = items.iter().any(|i| by_is(i, who));
    let mut fresh = vec![];
    for raw in list(p.get("findings")) {
        let f = match &raw { Value::String(s) => Some(json!({ "text": s })), Value::Object(_) | Value::Array(_) => Some(raw.clone()), _ => None };
        let body = match &f { Some(f) => slice16(&js_trim(&text(or(or(f.get("text"), f.get("finding")), f.get("issue")))), 1000), None => String::new() };
        let Some(f) = f.filter(|_| !body.is_empty()) else { return "Each finding needs a text: what is wrong and what to change.".into() };
        let blocking = matches!(f.get("blocking"), Some(Value::Bool(true))) || f.get("blocking").and_then(Value::as_str) == Some("true");
        let reason = slice16(&js_trim(&text(f.get("reason"))), 500);
        if blocking && earlier && reason.is_empty() {
            return format!("After your first review, a new blocking finding needs a reason (why it must block): add \"reason\" to \"{}\". Otherwise make it a nit (blocking: false), which is kept as a follow-up.", slice16(&body, 80));
        }
        fresh.push((body, slice16(&js_trim(&text(f.get("where"))), 200), blocking, reason));
    }
    for (body, wh, blocking, reason) in fresh {
        seq += 1.0;
        items.push(obj(vec![("id", some(format!("F{}", js_num(seq)))), ("text", some(body)), ("where", some(wh)), ("blocking", some(blocking)), ("reason", some(reason)), ("by", opt(who)), ("at", at.clone()), ("status", some("open"))]));
    }
    for id in &done { let i = &mut items[by_id[id]]; if st(i, "status") == "open" { set(i, "status", json!("claimed")); put(i, "claimedBy", opt(who)); } }
    for id in &verified { let i = &mut items[by_id[id]]; if st(i, "status") != "dismissed" { set(i, "status", json!("closed")); put(i, "closedBy", opt(who)); } }
    for id in &reopen { let i = &mut items[by_id[id]]; set(i, "status", json!("open")); let n = num(i.get("reopened").filter(|x| truthy(x))) + 1.0; set(i, "reopened", jn(n)); }
    for id in &dismiss { let i = &mut items[by_id[id]]; set(i, "status", json!("dismissed")); put(i, "closedBy", opt(who)); }
    set(l, "seq", jn(seq));
    String::new()
}

/// ledgerSummary(): one paragraph on the ledger, for escalations.
fn ledger_summary(r: &mut Value) -> String {
    let items = arr(ledger_of(r), "items").to_vec();
    if items.is_empty() { return String::new(); }
    let fmt = |i: &Value| format!("{} ({}): {}{}", js_string_opt(i.get("id")), if tr(i.get("blocking")) { "blocking" } else { "nit" }, slice16(&st(i, "text"), 140),
        if tr(i.get("reopened")) { format!(" [reopened {}x]", js_string_opt(i.get("reopened"))) } else { String::new() });
    let pick = |sts: &[&str]| items.iter().filter(|i| sts.contains(&st(i, "status").as_str())).collect::<Vec<_>>();
    let (open, claimed, closed) = (pick(&["open"]), pick(&["claimed"]), pick(&["closed", "dismissed"]));
    let mut out = vec![String::new(), "Review findings so far:".to_string()];
    if !open.is_empty() { out.push(format!("Still open: {}", open.iter().map(|&i| fmt(i)).collect::<Vec<_>>().join("; "))); }
    if !claimed.is_empty() { out.push(format!("Said to be fixed, not yet verified: {}", claimed.iter().map(|&i| fmt(i)).collect::<Vec<_>>().join("; "))); }
    out.push(format!("Closed: {}.", closed.len()));
    out.join("\n")
}

// ─── Routing internals ───

/// The run while a call works on it, with its context (newCtx()).
struct Run {
    r: Value,
    /// Every token the call has seen; `live` is `run.tokens`, in order.
    toks: Vec<Value>,
    live: Vec<usize>,
    ix: Ix,
    /// ctx.from: (name, label).
    from: Option<(String, String)>,
    summary: String,
    payload: Value,
    is_manager: bool,
    default_note: String,
    effects: Vec<Value>,
    errors: Vec<String>,
    keep_status: bool,
    /// meta.caps.maxHandoffs (the journal's: the factory's cap plus what a
    /// resume after the cap added); none: the factory's own.
    cap: Option<f64>,
}

impl Run {
    fn load(run: &Value) -> Run {
        let mut r = if run.is_object() { run.clone() } else { json!({}) };
        let toks = match std::mem::take(&mut r["tokens"]) { Value::Array(a) => a, _ => vec![] };
        let ix = index(&r["snapshot"]);
        Run {
            live: (0..toks.len()).collect(), toks, ix, r,
            from: None, summary: String::new(), payload: json!({}), is_manager: false, default_note: String::new(),
            effects: vec![], errors: vec![], keep_status: false, cap: None,
        }
    }

    fn into_run(mut self) -> Value {
        self.r["tokens"] = Value::Array(self.live.iter().map(|&i| self.toks[i].clone()).collect());
        self.r
    }

    fn find(&self, id: &str) -> Option<usize> { self.live.iter().copied().find(|&i| self.toks[i].get("id").and_then(Value::as_str) == Some(id)) }
    fn status(&self, t: usize) -> String { st(&self.toks[t], "status") }
    fn push_token(&mut self, tok: Value) -> usize { self.toks.push(tok); let i = self.toks.len() - 1; self.live.push(i); i }

    /// finishCtx().
    fn finish(mut self) -> Value {
        if !self.errors.is_empty() { return fail(&self.errors.join("\n")); }
        if !self.keep_status {
            let has = |sts: &[&str]| self.live.iter().any(|&i| sts.contains(&st(&self.toks[i], "status").as_str()));
            if has(&["work", "answer", "wait", "delegated"]) { set(&mut self.r, "status", json!("running")); }
            else if has(&["ask"]) { set(&mut self.r, "status", json!("waiting")); }
        }
        let effects = std::mem::take(&mut self.effects);
        json!({ "ok": true, "run": self.into_run(), "effects": effects })
    }

    fn log(&mut self, entry: Value) {
        let seq = num(self.r.get("seq")) + 1.0;
        set(&mut self.r, "seq", jn(seq));
        let mut m = Map::new();
        m.insert("seq".into(), jn(seq));
        if let Value::Object(e) = entry { m.extend(e); }
        let h = field(&mut self.r, "history");
        if !h.is_array() { *h = json!([]); }
        if let Value::Array(a) = h {
            a.push(Value::Object(m));
            if a.len() > HISTORY_MAX { let n = a.len() - HISTORY_MAX; a.drain(0..n); }
        }
    }

    fn new_id(&mut self, prefix: &str) -> String {
        let seq = num(self.r.get("seq")) + 1.0;
        set(&mut self.r, "seq", jn(seq));
        format!("{prefix}{}", js_num(seq))
    }

    fn inc_handoffs(&mut self) {
        let n = num(self.r.get("handoffs")) + 1.0;
        set(&mut self.r, "handoffs", jn(n));
    }

    /// The handoff cap (the factory's maxHandoffs): the run waits for the person.
    fn cap_check(&mut self) {
        let cap = match self.cap { Some(c) if c > 0.0 => jn(c), _ => self.r["snapshot"]["settings"]["caps"]["maxHandoffs"].clone() };
        if let Some(c) = cap.as_f64()
            && num(self.r.get("handoffs")) >= c
        {
            set(&mut self.r, "status", json!("waiting"));
            self.keep_status = true;
            self.effects.push(json!({ "type": "cap", "which": "maxHandoffs", "limit": cap }));
        }
    }

    fn finish_delegate(mut self, d: usize, p: &Value) -> Value {
        let who = st(&self.toks[d]["delegate"], "employeeId");
        let name = self.ix.name_of(Some(&who));
        let summary = js_trim(&text(p.get("summary")));
        let esc = tr(p.get("escalate"));
        if summary.is_empty() && !esc { return fail("Write a short summary of what you did (summary)."); }
        let lerr = apply_ledger(&mut self.r, Some(&who), p);
        if !lerr.is_empty() { return fail(&lerr); }
        self.inc_handoffs();
        let by = self.toks[d].get("delegatedBy").cloned();
        let m = self.live.iter().copied().find(|&i| self.toks[i].get("id") == by.as_ref());
        let did = self.toks[d].get("id").cloned();
        self.live.retain(|&i| self.toks[i].get("id") != did.as_ref());
        self.from = Some((name.clone(), "Job from the Manager".into()));
        self.summary = summary.clone();
        let reason = text(p.get("escalate"));
        self.log(json!({ "type": if esc { "delegate-escalated" } else { "delegate-done" }, "employeeId": who, "summary": if summary.is_empty() { reason.clone() } else { summary.clone() } }));
        if let Some(m) = m {
            let prev = self.toks[m].get("prevStatus").filter(|x| truthy(x)).cloned().unwrap_or(json!("work"));
            set(&mut self.toks[m], "status", prev);
            del(&mut self.toks[m], "prevStatus");
            let Some(node) = self.ix.node(&st(&self.toks[m], "nodeId")) else { return fail(UNDEF_ID) };
            let msg = if esc { format!("{name} could not do the job you handed out: {reason}") } else { format!("{name} finished the job you handed out: {summary}") };
            let mode = if self.status(m) == "answer" { "answer" } else { "work" };
            self.activate(m, &node, mode, &msg, "");
        }
        self.cap_check();
        self.finish()
    }

    fn deliver_notes(&mut self, node: &Value) {
        let edges = self.ix.notes_from.get(&st(node, "id")).cloned().unwrap_or_default();
        let notes = self.payload.get("notes").filter(|x| truthy(x)).cloned().unwrap_or(json!({}));
        let from = self.from.as_ref().map(|f| f.0.clone()).unwrap_or_default();
        for e in edges {
            let body = js_trim(&text(or(notes.get(st(&e, "id")), notes.get(st(&e, "target")))));
            if body.is_empty() { continue; }
            let to = self.ix.nodes.get(&st(&e, "target")).map(roster_of).unwrap_or_default();
            let list = field(field(&mut self.r, "inbox"), &to);
            if !tr(Some(list)) { *list = json!([]); }
            if let Value::Array(a) = list { a.push(json!({ "from": from, "label": or_str(e.get("noteLabel")), "text": body })); }
        }
    }

    fn traverse(&mut self, t: usize, e: &Value, who: Option<&str>, carry: &str) {
        if !self.errors.is_empty() { return; }
        let eid = st(e, "id");
        let max = num(e.get("maxRounds"));
        let used = num(self.r.get("edgeCounts").and_then(|c| c.get(&eid)));
        if max > 0.0 {
            if used >= max {
                let (src, dst) = (self.ix.node(&st(e, "source")), self.ix.node(&st(e, "target")));
                let what = format!("\"{}\" → \"{}\"", self.ix.label(src.as_ref()), self.ix.label(dst.as_ref()));
                let reopened = num(self.r.get("exhausted").and_then(|x| x.get(&eid)));
                if self.is_manager && reopened >= MANAGER_REOPEN_LIMIT {
                    self.errors.push(format!("The path {what} has used all {} of its rounds and is now closed. Choose another answer, finish, or ask the person.", js_num(max)));
                    return;
                }
                set(field(&mut self.r, "exhausted"), &eid, jn(reopened + 1.0));
                if self.is_manager {
                    // The Manager itself chose to go round again once more: allowed, counted.
                    set(field(&mut self.r, "edgeCounts"), &eid, jn(used + 1.0));
                } else {
                    self.log(json!({ "type": "limit", "edgeId": e.get("id").cloned().unwrap_or_default() }));
                    let ls = ledger_summary(&mut self.r);
                    self.escalate_token(t, &format!("The loop {what} reached its limit of {} round{}.{ls}", js_num(max), if max == 1.0 { "" } else { "s" }));
                    return;
                }
            } else {
                set(field(&mut self.r, "edgeCounts"), &eid, jn(used + 1.0));
            }
        } else {
            set(field(&mut self.r, "edgeCounts"), &eid, jn(used + 1.0));
        }
        let mut message = carry.to_string();
        if tr(e.get("requireNote")) {
            let dn = Value::String(self.default_note.clone());
            let notes = self.payload.get("notes").filter(|x| truthy(x));
            message = js_trim(&text(or(notes.and_then(|n| n.get(&eid)), Some(&dn))));
            if message.is_empty() {
                let label = e.get("noteLabel").filter(|x| truthy(x)).map(js_string).unwrap_or_else(|| "a note".into());
                self.errors.push(format!("This route needs a note: \"{label}\". Put it in notes.{eid}."));
                return;
            }
        }
        let node = self.ix.node(&st(e, "target"));
        self.arrive(t, node, who, &message);
    }

    fn arrive(&mut self, t: usize, node: Option<Value>, who: Option<&str>, message: &str) {
        if !self.errors.is_empty() { return; }
        let Some(node) = node else { self.errors.push("A route points at a node that does not exist.".into()); return };
        set(&mut self.toks[t], "nodeId", node.get("id").cloned().unwrap_or_default());
        match st(&node, "type").as_str() {
            "employee" | "manager" => {
                set(&mut self.toks[t], "status", json!("work"));
                del(&mut self.toks[t], "answerer");
                self.activate(t, &node, "work", message, "");
            }
            "decision" => {
                let answerer = answered_by(&node).or_else(|| who.map(String::from));
                let none = answerer.as_deref().is_none_or(str::is_empty);
                if none || answerer.as_deref() != who {
                    set(&mut self.toks[t], "status", json!("answer"));
                    set(&mut self.toks[t], "answerer", json!(if none { MANAGER_ID.to_string() } else { answerer.unwrap_or_default() }));
                    self.activate(t, &node, "answer", message, "");
                    return;
                }
                set(&mut self.toks[t], "status", json!("moving"));
                if !message.is_empty() { set(&mut self.toks[t], "carry", json!(message)); }
                self.eval_decision(t, &node, who);
            }
            "join" => self.handle_join(t, &node),
            "ask" => {
                set(&mut self.toks[t], "status", json!("ask"));
                let ask = node.get("ask");
                let ctx = self.base_context(message);
                self.effects.push(obj(vec![("type", some("ask")), ("tokenId", self.toks[t].get("id").cloned()), ("nodeId", node.get("id").cloned()), ("prompt", ask.and_then(|a| a.get("prompt")).cloned()), ("replies", ask.and_then(|a| a.get("replies")).cloned()), ("context", Some(ctx))]));
            }
            "wait" => {
                // Held until the runner says the wait is over; what came in is kept
                // for whoever comes next.
                set(&mut self.toks[t], "status", json!("wait"));
                let held = self.base_context(message);
                set(&mut self.toks[t], "held", held);
                self.effects.push(obj(vec![("type", some("wait")), ("tokenId", self.toks[t].get("id").cloned()), ("nodeId", node.get("id").cloned()), ("wait", node.get("wait").cloned()), ("ws", self.toks[t].get("ws").cloned())]));
            }
            _ => self.errors.push(format!("The route reached \"{}\", which cannot take control. (Only the Manager's Finish reaches End.)", self.ix.label(Some(&node)))),
        }
    }

    fn follow(&mut self, t: usize, node: &Value, h: &str, ask: &str, who: Option<&str>) {
        match self.ix.out(&st(node, "id"), h) {
            Some(e) => self.traverse(t, &e, who, ""),
            None => self.errors.push(format!("The answer \"{}\" to {ask} leads nowhere.", handle_label(node, h))),
        }
    }

    fn eval_decision(&mut self, t: usize, node: &Value, who: Option<&str>) {
        let nid = st(node, "id");
        let d = node.get("decision").cloned().unwrap_or_default();
        let raw: Option<Value> = self.payload.get("answers").filter(|x| truthy(x)).and_then(|a| a.get(&nid)).cloned();
        let question = d.get("question").filter(|x| truthy(x)).map(js_string).unwrap_or_else(|| self.ix.label(Some(node)));
        let ask = format!("\"{question}\"");
        let opts = arr(&d, "options").to_vec();
        let opt_list = opts.iter().map(|o| format!("\"{}\"", js_string_opt(o.get("id")))).collect::<Vec<_>>().join(", ");
        let match_option = |v: &Value| -> Option<String> {
            let s = js_trim(v.as_str()?).to_lowercase();
            opts.iter().find(|o| st(o, "id").to_lowercase() == s || st(o, "label").to_lowercase() == s).map(|o| st(o, "id")).filter(|id| !id.is_empty())
        };
        let kind = st(&d, "kind");

        if kind == "yesno" {
            let v = match &raw { Some(Value::Bool(true)) => "yes".to_string(), Some(Value::Bool(false)) => "no".into(), Some(Value::String(s)) => js_trim(s).to_lowercase(), _ => String::new() };
            if v != "yes" && v != "no" { self.errors.push(format!("Answer {ask} with \"yes\" or \"no\" in answers.{nid}.")); return; }
            // A review gate: "no" must say what blocks, "yes" must leave nothing blocking open.
            if tr(d.get("review")) {
                let items = items_mut(ledger_of(&mut self.r));
                let open_blocking: Vec<String> = items.iter().filter(|i| tr(i.get("blocking")) && st(i, "status") == "open").map(|i| js_string_opt(i.get("id"))).collect();
                if v == "no" && open_blocking.is_empty() {
                    self.errors.push("You did not approve, but there is no open blocking finding. Add one to findings (with blocking: true) saying what must change, reopen one that was not really fixed (reopen: [\"F1\"]), or approve.".into());
                    return;
                }
                if v == "yes" && !open_blocking.is_empty() {
                    self.errors.push(format!("You approved, but blocking findings are still open: {}. Verify them as fixed (verified: [...]), or answer no.", open_blocking.join(", ")));
                    return;
                }
                // Approving verifies whatever was claimed fixed.
                if v == "yes" {
                    for i in items.iter_mut() {
                        if tr(i.get("blocking")) && st(i, "status") == "claimed" { set(i, "status", json!("closed")); put(i, "closedBy", opt(who)); }
                    }
                }
            }
            return self.follow(t, node, &v, &ask, who);
        }
        if kind == "one" {
            let Some(id) = raw.as_ref().and_then(|r| match_option(r)) else {
                self.errors.push(format!("Answer {ask} with one of: {opt_list} in answers.{nid}."));
                return;
            };
            return self.follow(t, node, &id, &ask, who);
        }
        // many
        let picks: Vec<String> = match raw {
            None => {
                let Some(last) = self.r.get("lastPicks").and_then(|l| l.get(&nid)).filter(|x| truthy(x)) else {
                    self.errors.push(format!("Answer {ask} with a list of options ({opt_list}), or [] for none of these, in answers.{nid}."));
                    return;
                };
                match last { Value::Array(a) => a.iter().map(js_string).collect(), x => vec![js_string(x)] }
            }
            Some(raw) => {
                let list = if raw.as_str() == Some("none") { vec![] } else if let Value::Array(a) = raw { a } else { vec![raw] };
                let mut picks = vec![];
                for v in list {
                    let Some(id) = match_option(&v) else {
                        self.errors.push(format!("\"{}\" is not an option of {ask}. Options: {opt_list}.", js_string(&v)));
                        return;
                    };
                    if !picks.contains(&id) { picks.push(id); }
                }
                picks
            }
        };
        set(field(&mut self.r, "lastPicks"), &nid, json!(picks));
        if picks.is_empty() { return self.follow(t, node, "none", &ask, who); }
        self.fan_out(t, node, &picks, who);
    }

    fn fan_out(&mut self, t: usize, node: &Value, picks: &[String], who: Option<&str>) {
        let nid = st(node, "id");
        let Some(join) = self.ix.join_of.get(&nid).cloned() else {
            self.errors.push(format!("\"{}\" has no Join.", self.ix.label(Some(node))));
            return;
        };
        let gid = self.new_id("g");
        let ws0 = self.toks[t].get("ws").cloned().unwrap_or_default();
        let path0 = arr(&self.toks[t], "path").to_vec();
        set(field(&mut self.r, "groups"), &gid, json!({ "id": gid, "forkId": nid, "joinId": join.get("id").cloned().unwrap_or_default(), "parentWs": ws0, "parentPath": path0, "branches": [] }));
        let tid = self.toks[t].get("id").cloned();
        self.live.retain(|&x| self.toks[x].get("id") != tid.as_ref());
        let carry = self.toks[t].get("carry").filter(|x| truthy(x)).cloned();
        let mut children = vec![];
        for pick in picks {
            let mut ws = ws0.clone();
            if picks.len() > 1 {
                let w = self.new_id("w");
                let name = handle_label(node, pick);
                set(field(&mut self.r, "workspaces"), &w, json!({ "id": w, "parent": ws0, "name": name }));
                self.effects.push(json!({ "type": "workspace", "wsId": w, "parentWs": ws0, "name": name }));
                ws = json!(w);
            }
            let id = self.new_id("t");
            let mut path = path0.clone();
            path.push(json!(gid));
            let mut child = json!({ "id": id, "nodeId": nid, "ws": ws, "path": path, "status": "moving" });
            if let Some(c) = &carry { set(&mut child, "carry", c.clone()); }
            let ci = self.push_token(child);
            if let Some(Value::Array(b)) = self.r.get_mut("groups").and_then(|g| g.get_mut(&gid)).and_then(|g| g.get_mut("branches")) {
                b.push(json!({ "tokenId": id, "pick": pick, "ws": ws, "done": false, "summary": "" }));
            }
            children.push((ci, pick));
        }
        for (child, pick) in children {
            let Some(e) = self.ix.out(&nid, pick) else {
                self.errors.push(format!("The option \"{}\" leads nowhere.", handle_label(node, pick)));
                return;
            };
            self.traverse(child, &e, who, "");
        }
    }

    fn handle_join(&mut self, t: usize, join: &Value) {
        let jid = st(join, "id");
        let gid = arr(&self.toks[t], "path").last().cloned().unwrap_or_default();
        let gkey = js_string(&gid);
        let g = if truthy(&gid) { self.r.get("groups").and_then(|g| g.get(&gkey)).filter(|g| truthy(g)).cloned() } else { None };
        let out = self.ix.out(&jid, "out");
        if !g.is_some_and(|g| g.get("joinId") == join.get("id")) {
            // Arrived by "none of these": nothing to wait for.
            let Some(out) = out else { self.errors.push(format!("\"{}\" leads nowhere.", self.ix.label(Some(join)))); return };
            set(&mut self.toks[t], "status", json!("moving"));
            return self.traverse(t, &out, None, "");
        }
        let tid = self.toks[t].get("id").cloned();
        let line = if self.summary.is_empty() { None } else { Some(format!("{}: {}", self.from.as_ref().map(|f| f.0.as_str()).unwrap_or(""), self.summary)) };
        if let Some(Value::Array(bs)) = self.r.get_mut("groups").and_then(|g| g.get_mut(&gkey)).and_then(|g| g.get_mut("branches"))
            && let Some(b) = bs.iter_mut().find(|b| b.get("tokenId") == tid.as_ref())
        {
            set(b, "done", json!(true));
            if let Some(l) = line { set(b, "summary", json!(l)); }
        }
        set(&mut self.toks[t], "status", json!("join"));
        let g = self.r["groups"][gkey.as_str()].clone();
        let branches = arr(&g, "branches").to_vec();
        if !branches.iter().all(|b| tr(b.get("done"))) { return; }

        // Everyone is here: merge the branch workspaces and go on as one.
        let btids: Vec<Value> = branches.iter().filter_map(|b| b.get("tokenId").cloned()).collect();
        self.live.retain(|&x| !self.toks[x].get("id").is_some_and(|id| btids.contains(id)));
        if let Some(gs) = self.r.get_mut("groups") { del(gs, &gkey); }
        let parent_ws = g.get("parentWs").cloned().unwrap_or_default();
        let cont_id = self.new_id("t");
        let cont = self.push_token(json!({ "id": cont_id, "nodeId": jid, "ws": parent_ws, "path": g.get("parentPath").cloned().unwrap_or(json!([])), "status": "moving" }));
        let mut from: Vec<Value> = vec![];
        for b in &branches {
            let w = b.get("ws").cloned().unwrap_or_default();
            if w != parent_ws && !from.contains(&w) { from.push(w); }
        }
        if !from.is_empty() { self.effects.push(json!({ "type": "merge", "groupId": gid, "into": parent_ws, "from": from, "contTokenId": cont_id })); }
        if let Some(ws) = self.r.get_mut("workspaces") { for w in &from { del(ws, &js_string(w)); } }
        let joined = branches.iter().filter_map(|b| b.get("summary").filter(|x| truthy(x)).map(js_string)).collect::<Vec<_>>().join("\n");
        self.log(json!({ "type": "join", "nodeId": jid, "group": gid }));
        let Some(out) = out else { self.errors.push(format!("\"{}\" leads nowhere.", self.ix.label(Some(join)))); return };
        if !joined.is_empty() { self.summary = joined; }
        self.from = Some(("The branches".into(), self.ix.label(Some(join))));
        self.traverse(cont, &out, None, "");
    }

    fn escalate_token(&mut self, t: usize, reason: &str) {
        let path = arr(&self.toks[t], "path").to_vec();
        if let Some(first) = path.first() {
            // Leaving a fan-out early: stop the other branches, keep their work.
            let Some(outer) = self.r.get("groups").and_then(|g| g.get(js_string(first))).cloned() else { self.errors.push(UNDEF_ID.into()); return };
            let oid = outer.get("id").cloned();
            let in_group: Vec<Value> = self.r.get("groups").and_then(Value::as_object).map(|m| m.values()
                .filter(|g| g.get("id") == oid.as_ref() || oid.as_ref().is_some_and(|o| arr(g, "parentPath").contains(o)))
                .map(|g| g.get("id").cloned().unwrap_or_default()).collect()).unwrap_or_default();
            let tid = self.toks[t].get("id").cloned();
            let others: Vec<usize> = self.live.iter().copied().filter(|&x| self.toks[x].get("id") != tid.as_ref() && arr(&self.toks[x], "path").iter().any(|p| in_group.contains(p))).collect();
            if !others.is_empty() {
                let ids: Vec<Value> = others.iter().map(|&x| self.toks[x].get("id").cloned().unwrap_or_default()).collect();
                self.effects.push(json!({ "type": "cancel", "tokenIds": ids, "reason": reason }));
            }
            let parent_ws = outer.get("parentWs").cloned();
            let mut from: Vec<Value> = vec![];
            for x in std::iter::once(t).chain(others.iter().copied()) {
                let w = self.toks[x].get("ws").cloned().unwrap_or_default();
                if Some(&w) != parent_ws.as_ref() && !from.contains(&w) { from.push(w); }
            }
            self.live.retain(|x| !others.contains(x));
            if !from.is_empty() {
                self.effects.push(obj(vec![("type", some("merge")), ("groupId", oid.clone()), ("into", parent_ws.clone()), ("from", some(from.clone())), ("contTokenId", tid), ("abort", some(true))]));
            }
            if let Some(ws) = self.r.get_mut("workspaces") { for w in &from { del(ws, &js_string(w)); } }
            if let Some(gs) = self.r.get_mut("groups") { for g in &in_group { del(gs, &js_string(g)); } }
            put(&mut self.toks[t], "ws", parent_ws);
            set(&mut self.toks[t], "path", outer.get("parentPath").cloned().unwrap_or(json!([])));
        }
        let Some(mgr) = self.ix.manager.clone() else { self.errors.push(UNDEF_ID.into()); return };
        set(&mut self.toks[t], "nodeId", mgr.get("id").cloned().unwrap_or_default());
        set(&mut self.toks[t], "status", json!("work"));
        del(&mut self.toks[t], "answerer");
        self.activate(t, &mgr, "work", "", reason);
    }

    fn base_context(&self, message: &str) -> Value {
        json!({
            "from": self.from.as_ref().map(|(n, l)| json!({ "name": n, "label": l })).unwrap_or_default(),
            "summary": self.summary,
            "message": message,
        })
    }

    fn activate(&mut self, t: usize, node: &Value, mode: &str, message: &str, escalation: &str) {
        let who: Option<String> = if mode == "answer" { self.toks[t].get("answerer").and_then(Value::as_str).map(String::from) } else { Some(roster_of(node)) };
        let carried = self.toks[t].get("carry").filter(|x| truthy(x)).map(js_string).unwrap_or_default();
        del(&mut self.toks[t], "carry");
        let mut notes = json!([]);
        if mode == "work" {
            let w = who.clone().unwrap_or_default();
            if let Some(x) = self.r.get("inbox").and_then(|i| i.get(&w)).filter(|x| truthy(x)) { notes = x.clone(); }
            if let Some(i) = self.r.get_mut("inbox") { del(i, &w); }
        }
        let mut ctx = self.base_context(if message.is_empty() { &carried } else { message });
        set(&mut ctx, "notes", notes);
        set(&mut ctx, "escalation", json!(escalation));
        self.effects.push(obj(vec![
            ("type", some("activate")),
            ("tokenId", self.toks[t].get("id").cloned()),
            ("nodeId", node.get("id").cloned()),
            ("employeeId", who.map(Value::from)),
            ("mode", some(mode)),
            ("ws", self.toks[t].get("ws").cloned()),
            ("context", Some(ctx)),
        ]));
    }
}
