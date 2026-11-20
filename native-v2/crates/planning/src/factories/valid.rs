//! Factories: the factory's own rules, as the web has them
//! (src/factory-model.js normalizeFactory, validateFactory and the graph
//! helpers, with mentionsInFactory from src/factory-skills.js and
//! normalizeTemplate from src/factory-cr.js), so the editor and the run
//! engine check a factory exactly as the server does.
//!
//! Ported rule for rule: the same codes, wording and order of issues. JS
//! semantics kept where they show: `||` defaults, `Number()` coercion of
//! strings, `Math.round` (half up), `String()` of numbers, Map/Set insertion
//! order (kept with Vecs, never with hash order). Strings are cut by chars
//! (the JS cuts by UTF-16 units; the two differ only past the BMP).

use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

// ─── Constants ───

const NODE_TYPES: [&str; 8] = ["start", "manager", "employee", "decision", "join", "ask", "wait", "end"];
const WAIT_KINDS: [&str; 4] = ["time", "until", "command", "signal"];
const AGENT_TYPES: [&str; 2] = ["manager", "employee"];
const DECISION_KINDS: [&str; 3] = ["yesno", "one", "many"];
const EDGE_KINDS: [&str; 2] = ["flow", "note"];
const MEMORY_MODES: [&str; 2] = ["keep", "fresh"];
const MANAGER_MODES: [&str; 2] = ["normal", "bypass"];
const MERGE_MODES: [&str; 2] = ["ask", "auto"];
const TEAM_MODES: [&str; 2] = ["ask", "bypass"];
const MANAGER_ID: &str = "manager";

// LIMITS
const LIM_NODES: usize = 200;
const LIM_EDGES: usize = 600;
const LIM_ROSTER: usize = 50;
const LIM_OPTIONS: usize = 12;
const LIM_NAME: usize = 80;
const LIM_LABEL: usize = 120;
const LIM_TEXT: usize = 20_000;
const LIM_SHORT: usize = 500;
const LIM_ROUNDS: f64 = 100.0;

// DEFAULT_CAPS
const CAP_HANDOFFS: f64 = 200.0;
const CAP_MINUTES: f64 = 8.0 * 60.0;
const CAP_TOKENS: f64 = 0.0;

/// The keys of PERMISSION_PRESETS (only the keys matter to normalize).
const PRESET_KEYS: [&str; 4] = ["git-read", "tests", "packages", "web"];
const DEFAULT_PRESETS: [&str; 2] = ["git-read", "tests"];
const RESERVED_HANDLES: [&str; 8] = ["out", "finish", "note", "none", "yes", "no", "done", "timeout"];
const MAX_SECRETS: usize = 20;

// factory-cr.js CR_LIMITS (the ones normalizeTemplate uses)
const CR_SECTIONS: usize = 12;
const CR_LABEL: usize = 60;
const CR_HINT: usize = 300;

/// DEFAULT_CR_TEMPLATE: (id, label, required, list, hint).
const DEFAULT_CR_TEMPLATE: [(&str, &str, bool, bool, &str); 6] = [
    ("goal", "Goal", true, false, "Why this change, and what it should achieve. One or two sentences."),
    ("current", "Current situation", false, false, "What exists or happens today, and what is wrong or missing. For a new project: what there is to start from."),
    ("expected", "Expected result", true, false, "What it should do when finished: behaviour, screens, data, APIs. Concrete examples help."),
    ("done", "Done when", true, true, "One check per line. The reviewers hold the work to these, and the Manager finishes only when they all pass."),
    ("scope", "Out of scope", false, true, "What not to do or touch, one per line."),
    ("notes", "Constraints and references", false, false, "Tech to use or avoid, conventions, links, anything the team should know."),
];

static NULL: Value = Value::Null;

// ─── JS semantics ───

/// `v[k]` on an object; None (undefined) for anything else.
fn get<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    match v { Value::Object(m) => m.get(k), _ => None }
}
/// `v[k]`, with undefined as null, for chained lookups.
fn at<'a>(v: &'a Value, k: &str) -> &'a Value { get(v, k).unwrap_or(&NULL) }
/// `v && typeof v === 'object'` (arrays pass too, as in JS).
fn is_obj(v: &Value) -> bool { matches!(v, Value::Object(_) | Value::Array(_)) }
/// `Array.isArray(v) ? v : []`.
fn arr_of(v: Option<&Value>) -> &[Value] {
    match v { Some(Value::Array(a)) => a.as_slice(), _ => &[] }
}
/// `!!v`.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => { let x = n.as_f64().unwrap_or(0.0); x != 0.0 && !x.is_nan() }
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}
/// JS `\s` / String.prototype.trim whitespace (WhiteSpace + LineTerminator).
fn is_ws(c: char) -> bool { (c.is_whitespace() && c != '\u{85}') || c == '\u{FEFF}' }
/// A JS LineTerminator (what `.` does not match).
fn is_lt(c: char) -> bool { matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}') }
/// `\w` (ASCII).
fn is_word(c: char) -> bool { c.is_ascii_alphanumeric() || c == '_' }
fn trim(s: &str) -> &str { s.trim_matches(is_ws) }
/// `s.slice(0, max)`, by chars.
fn take(s: &str, max: usize) -> String { s.chars().take(max).collect() }

/// `String(x)` for a number.
fn num_str(x: f64) -> String {
    if x.is_nan() { return "NaN".into(); }
    if x.is_infinite() { return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() }; }
    if x == 0.0 { return "0".into(); }
    let a = x.abs();
    if !(1e-6..1e21).contains(&a) {
        let e = format!("{x:e}");
        return match e.split_once('e') {
            Some((m, ex)) if !ex.starts_with('-') => format!("{m}e+{ex}"),
            _ => e,
        };
    }
    format!("{x}")
}

/// `String(v)` (null as "null"; array items null as "").
fn js_string(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => num_str(n.as_f64().unwrap_or(f64::NAN)),
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(|x| if x.is_null() { String::new() } else { js_string(x) }).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

/// `typeof v === 'string' ? v : v == null ? '' : String(v)` (factory-cr.js str).
fn str_full(v: Option<&Value>) -> String {
    match v { None | Some(Value::Null) => String::new(), Some(x) => js_string(x) }
}
/// factory-model.js `str(v, max)`.
fn str_(v: Option<&Value>, max: usize) -> String { take(&str_full(v), max) }

/// `Number(s)` for a string.
fn str_to_number(s: &str) -> f64 {
    let t = trim(s);
    if t.is_empty() { return 0.0; }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (p, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if let Some(rest) = t.strip_prefix(p) {
            if rest.is_empty() { return f64::NAN; }
            let mut acc = 0.0;
            for c in rest.chars() {
                match c.to_digit(radix) { Some(d) => acc = acc * radix as f64 + d as f64, None => return f64::NAN }
            }
            return acc;
        }
    }
    if !t.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-')) { return f64::NAN; }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// `Number(v)` (`+v`); None is undefined.
fn to_number(v: Option<&Value>) -> f64 {
    match v {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(b)) => if *b { 1.0 } else { 0.0 },
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => str_to_number(s),
        Some(a @ Value::Array(_)) => str_to_number(&js_string(a)),
        Some(Value::Object(_)) => f64::NAN,
    }
}

/// `Math.round` (halves round up, towards +Infinity).
fn round(x: f64) -> f64 {
    if !x.is_finite() { return x; }
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// A number as JSON the way JSON.stringify writes it (integers without ".0").
fn num_json(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() <= 9_007_199_254_740_991.0 { json!(x as i64) } else { json!(x) }
}

/// `intIn(v, lo, hi, dflt)`.
fn int_in(v: Option<&Value>, lo: f64, hi: f64, dflt: f64) -> Value {
    let n = to_number(v);
    num_json(if n.is_finite() { hi.min(lo.max(round(n))) } else { dflt })
}

/// `oneOf(v, list, dflt)`.
fn one_of(v: Option<&Value>, list: &[&str], dflt: &str) -> String {
    match v.and_then(Value::as_str) { Some(x) if list.contains(&x) => x.to_string(), _ => dflt.to_string() }
}

/// `ID_RE` / `SECRET_ID_RE`: /^[A-Za-z0-9_-]{1,64}$/.
fn id_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

/// `genId(prefix)`: the prefix and 6 random bytes as 12 hex digits.
fn gen_id(prefix: &str) -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    h.write_u128(nanos);
    format!("{prefix}{:012x}", h.finish() & 0xFFFF_FFFF_FFFF)
}

/// `cleanId(v, prefix)`.
fn clean_id(v: Option<&Value>, prefix: &str) -> String {
    match v.and_then(Value::as_str) { Some(s) if id_ok(s) => s.to_string(), _ => gen_id(prefix) }
}

/// `strList(v, max, each)`.
fn str_list(v: Option<&Value>, max: usize, each: usize) -> Vec<String> {
    arr_of(v).iter().filter_map(|x| x.as_str().filter(|s| !s.is_empty())).take(max).map(|s| take(s, each)).collect()
}

/// `Array.isArray(v) ? v.filter(k => hasOwnProperty(PERMISSION_PRESETS, k)) : dflt`
/// (the key is coerced to a string, the kept item is the original).
fn presets_of(v: Option<&Value>, dflt: &[&str]) -> Value {
    match v {
        Some(Value::Array(a)) => Value::Array(a.iter().filter(|k| PRESET_KEYS.contains(&js_string(k).as_str())).cloned().collect()),
        _ => json!(dflt),
    }
}

// ─── Normalize ───

/// normalizeSecretIds(): settings.secrets as a list of secret ids.
fn normalize_secret_ids(v: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for x in arr_of(v) {
        let id = match x {
            Value::String(s) => s.as_str(),
            o if is_obj(o) => get(o, "id").and_then(Value::as_str).unwrap_or(""),
            _ => "",
        };
        if id_ok(id) && !out.iter().any(|o| o == id) { out.push(id.to_string()); }
        if out.len() >= MAX_SECRETS { break; }
    }
    out
}

/// normalizeLimits(): a run's usage limits, cleaned.
fn normalize_limits(l: Option<&Value>) -> Value {
    let x = l.unwrap_or(&NULL);
    let pct = |v: Option<&Value>, lo: f64, dflt: f64| {
        let n = round(to_number(v));
        num_json(if n.is_finite() && n > 0.0 { 100f64.min(lo.max(n)) } else { dflt })
    };
    let n = round(to_number(get(x, "maxParallel")));
    json!({
        "pauseAt5h": pct(get(x, "pauseAt5h"), 50.0, 100.0),
        "pauseAtWeek": pct(get(x, "pauseAtWeek"), 50.0, 100.0),
        "maxParallel": num_json(if n.is_finite() && n > 0.0 { 20f64.min(n) } else { 0.0 }),
        "oneAtATimeAbove": pct(get(x, "oneAtATimeAbove"), 10.0, 0.0),
    })
}

/// factory-cr.js slug().
fn slug(v: Option<&Value>) -> String {
    let mut out = String::new();
    let mut in_run = false;
    for c in str_full(v).to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() { out.push(c); in_run = false; } else if !in_run { out.push('-'); in_run = true; }
    }
    let t = out.strip_prefix('-').unwrap_or(&out);
    let t = t.strip_suffix('-').unwrap_or(t);
    take(t, 30)
}

/// factory-cr.js normalizeTemplate(): a factory's CR template, cleaned.
fn normalize_template(t: Option<&Value>) -> Value {
    let default = || Value::Array(DEFAULT_CR_TEMPLATE.iter().map(|(id, label, required, list, hint)| json!({ "id": id, "label": label, "required": required, "list": list, "hint": hint })).collect());
    let list = match t { Some(Value::Array(a)) if !a.is_empty() => a, _ => return default() };
    let mut out = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    for raw in list.iter().take(CR_SECTIONS) {
        if !is_obj(raw) { continue; }
        let label = take(trim(&str_full(get(raw, "label"))), CR_LABEL);
        if label.is_empty() { continue; }
        let mut id = slug(get(raw, "id"));
        if id.is_empty() { id = slug(Some(&Value::String(label.clone()))); }
        if id.is_empty() { id = "s".into(); }
        while seen.contains(&id) || id == "title" { id.push_str("-2"); }
        seen.insert(id.clone());
        out.push(json!({ "id": id, "label": label, "hint": take(trim(&str_full(get(raw, "hint"))), CR_HINT), "required": truthy(get(raw, "required")), "list": truthy(get(raw, "list")) }));
    }
    if out.is_empty() { default() } else { Value::Array(out) }
}

/// normalizeChoices(): decision options / ask replies.
fn normalize_choices(list: Option<&Value>) -> Value {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = vec![];
    for o in arr_of(list).iter().take(LIM_OPTIONS) {
        if !is_obj(o) { continue; }
        let mut id = match get(o, "id").and_then(Value::as_str) { Some(s) if id_ok(s) => s.to_string(), _ => gen_id("o_") };
        while seen.contains(&id) { id = gen_id("o_"); }
        seen.insert(id.clone());
        out.push(json!({ "id": id, "label": trim(&str_(get(o, "label"), LIM_LABEL)) }));
    }
    Value::Array(out)
}

/// `^([01]\d|2[0-3]):[0-5]\d$`
fn clock_ok(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 5 && b[2] == b':' && b[4].is_ascii_digit() && (b'0'..=b'5').contains(&b[3])
        && ((matches!(b[0], b'0' | b'1') && b[1].is_ascii_digit()) || (b[0] == b'2' && (b'0'..=b'3').contains(&b[1])))
}

/// normalizeFactory(): shape an untrusted definition into the known form.
/// Never fails and never "fixes" the graph: wrong wiring is kept so
/// validate() can say what is wrong and where. Only types, lengths and
/// missing ids are corrected.
pub(crate) fn normalize(input: &Value) -> Value {
    let f = input;
    let s = at(f, "settings");
    let caps = at(s, "caps");
    let gk = get(s, "gatekeeper");
    let gk_model = if truthy(gk) { gk.and_then(|g| get(g, "model")) } else { gk };
    let gk_model = { let m = str_(gk_model, 60); if m.is_empty() { "haiku".to_string() } else { m } };
    let tick = { let n = round(to_number(get(s, "tickMinutes"))); if n.is_finite() && n > 0.0 { 1440f64.min(5f64.max(n)) } else { 0.0 } };
    let name = { let n = trim(&str_(get(f, "name"), LIM_NAME)).to_string(); if n.is_empty() { "Untitled factory".to_string() } else { n } };
    let rules: Vec<String> = str_list(get(s, "rules"), 50, 200).iter().map(|x| trim(x).to_string()).filter(|x| !x.is_empty()).collect();

    let mut roster = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    for r in arr_of(get(f, "roster")).iter().take(LIM_ROSTER) {
        if !is_obj(r) { continue; }
        let mut id = if get(r, "id").and_then(Value::as_str) == Some(MANAGER_ID) { MANAGER_ID.to_string() } else { clean_id(get(r, "id"), "emp_") };
        while seen.contains(&id) { id = gen_id("emp_"); }
        seen.insert(id.clone());
        let tools = at(r, "tools");
        roster.push(json!({
            "id": id,
            "name": trim(&str_(get(r, "name"), LIM_NAME)),
            "persona": str_(get(r, "persona"), LIM_TEXT),
            "model": str_(get(r, "model"), 60),
            "effort": str_(get(r, "effort"), 20),
            "memory": one_of(get(r, "memory"), &MEMORY_MODES, "keep"),
            "tools": { "allow": str_list(get(tools, "allow"), 100, 200), "deny": str_list(get(tools, "deny"), 100, 200) },
            "skills": str_list(get(r, "skills"), 50, 100),
            "presets": presets_of(get(r, "presets"), &[]),
        }));
    }

    let mut nodes = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    for n in arr_of(get(f, "nodes")).iter().take(LIM_NODES) {
        if !is_obj(n) { continue; }
        let mut id = clean_id(get(n, "id"), "n_");
        while seen.contains(&id) { id = gen_id("n_"); }
        seen.insert(id.clone());
        let pos = at(n, "position");
        let coord = |k: &str| { let x = to_number(get(pos, k)); num_json(if x.is_finite() { x } else { 0.0 }) };
        let ty = get(n, "type").and_then(Value::as_str).map(|t| take(t, 20)).unwrap_or_default();
        let mut node = json!({ "id": id, "type": ty, "label": str_(get(n, "label"), LIM_LABEL), "position": { "x": coord("x"), "y": coord("y") } });
        match ty.as_str() {
            "employee" => { node["employeeId"] = json!(get(n, "employeeId").and_then(Value::as_str).unwrap_or("")); }
            "decision" => {
                let d = at(n, "decision");
                let kind = one_of(get(d, "kind"), &DECISION_KINDS, "yesno");
                let options = if kind == "yesno" { json!([]) } else { normalize_choices(get(d, "options")) };
                let answered = match get(d, "answeredBy").and_then(Value::as_str) { Some(a) if !a.is_empty() => json!(a), _ => Value::Null };
                node["decision"] = json!({ "kind": kind, "question": str_(get(d, "question"), LIM_TEXT), "options": options, "answeredBy": answered, "review": truthy(get(d, "review")) });
            }
            "ask" => {
                let a = at(n, "ask");
                node["ask"] = json!({ "prompt": str_(get(a, "prompt"), LIM_TEXT), "replies": normalize_choices(get(a, "replies")) });
            }
            "wait" => {
                let w = at(n, "wait");
                let at_s = if truthy(get(w, "at")) { str_full(get(w, "at")) } else { String::new() };
                node["wait"] = json!({
                    "kind": one_of(get(w, "kind"), &WAIT_KINDS, "time"),
                    "minutes": int_in(get(w, "minutes"), 1.0, 10080.0, 10.0),
                    "at": if clock_ok(&at_s) { at_s } else { "09:00".to_string() },
                    "command": str_(get(w, "command"), 1000),
                    "every": int_in(get(w, "every"), 15.0, 3600.0, 60.0),
                    "timeout": int_in(get(w, "timeout"), 1.0, 10080.0, 60.0),
                    "what": str_(get(w, "what"), LIM_TEXT),
                });
            }
            "join" => { node["join"] = json!({ "forkId": get(at(n, "join"), "forkId").and_then(Value::as_str).unwrap_or("") }); }
            _ => {}
        }
        nodes.push(node);
    }

    let mut edges = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    for e in arr_of(get(f, "edges")).iter().take(LIM_EDGES) {
        if !is_obj(e) { continue; }
        let mut id = clean_id(get(e, "id"), "e_");
        while seen.contains(&id) { id = gen_id("e_"); }
        seen.insert(id.clone());
        let kind = one_of(get(e, "kind"), &EDGE_KINDS, "flow");
        let flow = kind == "flow";
        let handle = if kind == "note" { "note".to_string() } else {
            match get(e, "sourceHandle").and_then(Value::as_str) { Some(h) if !h.is_empty() => take(h, 64), _ => "out".to_string() }
        };
        let mr = get(e, "maxRounds");
        let limited = flow && !matches!(mr, None | Some(Value::Null)) && mr.and_then(Value::as_str) != Some("") && to_number(mr) > 0.0;
        edges.push(json!({
            "id": id,
            "kind": kind,
            "source": get(e, "source").and_then(Value::as_str).unwrap_or(""),
            "sourceHandle": handle,
            "target": get(e, "target").and_then(Value::as_str).unwrap_or(""),
            "requireNote": flow && truthy(get(e, "requireNote")),
            "noteLabel": str_(get(e, "noteLabel"), LIM_SHORT),
            "maxRounds": if limited { int_in(mr, 1.0, LIM_ROUNDS, 1.0) } else { json!(0) },
        }));
    }

    json!({
        "id": match get(f, "id").and_then(Value::as_str) { Some(i) if id_ok(i) => i, _ => "" },
        "name": name,
        "description": str_(get(f, "description"), LIM_TEXT),
        "version": int_in(get(f, "version"), 0.0, 9_007_199_254_740_991.0, 0.0),
        "createdAt": get(f, "createdAt").and_then(Value::as_str).unwrap_or(""),
        "updatedAt": get(f, "updatedAt").and_then(Value::as_str).unwrap_or(""),
        "settings": {
            "managerMode": one_of(get(s, "managerMode"), &MANAGER_MODES, "normal"),
            "mergeMode": one_of(get(s, "mergeMode"), &MERGE_MODES, "ask"),
            "setupCommand": str_(get(s, "setupCommand"), LIM_SHORT),
            "autoRecover": truthy(get(s, "autoRecover")),
            "teamMode": one_of(get(s, "teamMode"), &TEAM_MODES, "ask"),
            "presets": presets_of(get(s, "presets"), &DEFAULT_PRESETS),
            "rules": rules,
            "managerRules": match get(s, "managerRules") { None => true, v => truthy(v) },
            "crTemplate": normalize_template(get(s, "crTemplate")),
            "limits": normalize_limits(get(s, "limits")),
            "tickMinutes": num_json(tick),
            "secrets": normalize_secret_ids(get(s, "secrets")),
            "gatekeeper": { "enabled": truthy(gk) && truthy(gk.and_then(|g| get(g, "enabled"))), "model": gk_model },
            "caps": {
                "maxHandoffs": int_in(get(caps, "maxHandoffs"), 1.0, 10_000.0, CAP_HANDOFFS),
                "maxMinutes": int_in(get(caps, "maxMinutes"), 1.0, 7.0 * 24.0 * 60.0, CAP_MINUTES),
                "tokenBudget": int_in(get(caps, "tokenBudget"), 0.0, 1e12, CAP_TOKENS),
            },
        },
        "roster": roster,
        "nodes": nodes,
        "edges": edges,
    })
}

// ─── Graph helpers (shared with the run engine) ───

/// A field of a normalized object as &str ("" when missing).
fn gs<'a>(v: &'a Value, k: &str) -> &'a str { v.get(k).and_then(Value::as_str).unwrap_or("") }
/// A list field of a normalized object ([] when missing).
fn ga<'a>(v: &'a Value, k: &str) -> &'a [Value] { arr_of(v.get(k)) }

/// outputsOf(): the output handles a node offers; [] for End and broken nodes.
fn outputs_of(node: &Value) -> Vec<String> {
    let ids = |list: &[Value]| list.iter().map(|o| gs(o, "id").to_string()).collect::<Vec<_>>();
    match gs(node, "type") {
        "start" | "employee" | "join" => vec!["out".into()],
        "manager" => vec!["out".into(), "finish".into()],
        "decision" => {
            let d = at(node, "decision");
            match gs(d, "kind") {
                "yesno" => vec!["yes".into(), "no".into()],
                "many" => { let mut v = ids(ga(d, "options")); v.push("none".into()); v }
                _ => ids(ga(d, "options")),
            }
        }
        "ask" => ids(ga(at(node, "ask"), "replies")),
        "wait" => { let k = gs(at(node, "wait"), "kind"); if k == "command" || k == "signal" { vec!["done".into(), "timeout".into()] } else { vec!["out".into()] } }
        _ => vec![],
    }
}

/// requiredOutputsOf(): which outputs must be wired (the Manager's `out` is optional).
fn required_outputs_of(node: &Value) -> Vec<String> {
    if gs(node, "type") == "manager" { vec!["finish".into()] } else { outputs_of(node) }
}

/// A set of node ids that remembers insertion order (a JS Set).
#[derive(Debug, Clone, Default)]
pub(crate) struct IdSet { order: Vec<String>, set: HashSet<String> }

impl IdSet {
    /// Whether the id is in the set.
    pub(crate) fn has(&self, id: &str) -> bool { self.set.contains(id) }
    /// The ids in the order they were added.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &str> { self.order.iter().map(String::as_str) }
    /// True when there is nothing in the set.
    pub(crate) fn is_empty(&self) -> bool { self.order.is_empty() }
    /// Add an id (no-op when present).
    pub(crate) fn insert(&mut self, id: &str) {
        if self.set.insert(id.to_string()) { self.order.push(id.to_string()); }
    }
    /// Remove an id.
    pub(crate) fn remove(&mut self, id: &str) {
        if self.set.remove(id) { self.order.retain(|x| x != id); }
    }
}

/// flowGraph(): the flow edges of a normalized factory whose source and
/// target both exist, as adjacency lists keyed by node id.
#[derive(Debug, Clone, Default)]
pub(crate) struct FlowGraph {
    /// Every node id, in the factory's node order (the JS Map's key order).
    pub(crate) order: Vec<String>,
    /// Node id -> flow edges leaving it (edge objects, in the factory's edge order).
    pub(crate) out: HashMap<String, Vec<Value>>,
    /// Node id -> flow edges arriving at it.
    pub(crate) inn: HashMap<String, Vec<Value>>,
}

/// flowGraph(f): every flow edge of a normalized factory (see `normalize`)
/// whose source and target both exist.
pub(crate) fn flow_graph(f: &Value) -> FlowGraph { flow_graph_without(f, |_| false) }

/// flowGraph(f, { exclude }): as `flow_graph`, leaving out the edges for
/// which `exclude` is true.
pub(crate) fn flow_graph_without(f: &Value, exclude: impl Fn(&Value) -> bool) -> FlowGraph {
    let mut g = FlowGraph::default();
    for n in ga(f, "nodes") {
        let id = gs(n, "id");
        if g.out.contains_key(id) { continue; }
        g.order.push(id.to_string());
        g.out.insert(id.to_string(), vec![]);
        g.inn.insert(id.to_string(), vec![]);
    }
    for e in ga(f, "edges") {
        let (src, tgt) = (gs(e, "source"), gs(e, "target"));
        if gs(e, "kind") != "flow" || !g.out.contains_key(src) || !g.out.contains_key(tgt) { continue; }
        if exclude(e) { continue; }
        if let Some(l) = g.out.get_mut(src) { l.push(e.clone()); }
        if let Some(l) = g.inn.get_mut(tgt) { l.push(e.clone()); }
    }
    g
}

/// reachable(graph, roots, { backward, stopAt }): node ids reachable from
/// `roots` following edges forward (or backward), in the JS visiting order.
/// Roots not in the graph are ignored; nodes in `stop_at` are included but
/// not walked past.
pub(crate) fn reachable(g: &FlowGraph, roots: &[&str], backward: bool, stop_at: Option<&HashSet<String>>) -> IdSet {
    let mut seen = IdSet::default();
    let mut stack: Vec<String> = roots.iter().filter(|r| g.out.contains_key(**r)).map(|r| r.to_string()).collect();
    while let Some(id) = stack.pop() {
        if seen.has(&id) { continue; }
        seen.insert(&id);
        if stop_at.is_some_and(|s| s.contains(&id)) { continue; }
        let list = if backward { g.inn.get(&id) } else { g.out.get(&id) };
        for e in list.map(Vec::as_slice).unwrap_or(&[]) {
            stack.push(gs(e, if backward { "source" } else { "target" }).to_string());
        }
    }
    seen
}

/// cycles(): strongly connected components (Tarjan, iterative) that are
/// real cycles: more than one node, or one node with an edge to itself.
fn cycles(g: &FlowGraph) -> Vec<Vec<String>> {
    let mut index = 0usize;
    let mut idx: HashMap<String, usize> = HashMap::new();
    let mut low: HashMap<String, usize> = HashMap::new();
    let mut on_stack: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = vec![];
    let mut result = vec![];
    let none: Vec<Value> = vec![];
    for start in &g.order {
        if idx.contains_key(start) { continue; }
        let mut work: Vec<(String, usize)> = vec![(start.clone(), 0)];
        idx.insert(start.clone(), index);
        low.insert(start.clone(), index);
        index += 1;
        stack.push(start.clone());
        on_stack.insert(start.clone());
        while let Some(frame) = work.last_mut() {
            let (v, i) = (frame.0.clone(), frame.1);
            frame.1 += 1;
            let edges = g.out.get(&v).unwrap_or(&none);
            if i < edges.len() {
                let w = gs(&edges[i], "target").to_string();
                if !idx.contains_key(&w) {
                    idx.insert(w.clone(), index);
                    low.insert(w.clone(), index);
                    index += 1;
                    stack.push(w.clone());
                    on_stack.insert(w.clone());
                    work.push((w, 0));
                } else if on_stack.contains(&w) {
                    let m = low[&v].min(idx[&w]);
                    low.insert(v, m);
                }
            } else {
                work.pop();
                if let Some(parent) = work.last() {
                    let m = low[&parent.0].min(low[&v]);
                    low.insert(parent.0.clone(), m);
                }
                if low[&v] == idx[&v] {
                    let mut comp = vec![];
                    while let Some(w) = stack.pop() {
                        on_stack.remove(&w);
                        let done = w == v;
                        comp.push(w);
                        if done { break; }
                    }
                    let self_loop = comp.len() == 1 && edges.iter().any(|e| gs(e, "target") == v);
                    if comp.len() > 1 || self_loop { result.push(comp); }
                }
            }
        }
    }
    result
}

/// forkRegion(): every node reachable from a pick-many fan-out's branch
/// targets without passing through its Join.
fn fork_region(g: &FlowGraph, fork_id: &str, join_id: &str) -> IdSet {
    let targets: Vec<&str> = g.out.get(fork_id).map(Vec::as_slice).unwrap_or(&[]).iter()
        .filter(|e| gs(e, "sourceHandle") != "none").map(|e| gs(e, "target")).filter(|t| *t != join_id).collect();
    let stop: HashSet<String> = HashSet::from([join_id.to_string()]);
    let mut region = reachable(g, &targets, false, Some(&stop));
    region.remove(join_id);
    region
}

/// sameCycle(): whether a limited edge's source and target are in the same cycle.
fn same_cycle(g: &FlowGraph, e: &Value) -> bool {
    let (src, tgt) = (gs(e, "source"), gs(e, "target"));
    src == tgt || reachable(g, &[tgt], false, None).has(src)
}

/// answerOrigins(...).has('?'): whether some path into a decision (walking
/// back through unassigned decisions) arrives from a node that cannot answer
/// it (Start, a Join, an Ask me, a Wait).
fn answer_has_gap(g: &FlowGraph, nodes: &HashMap<&str, &Value>, decision_id: &str) -> bool {
    let mut seen: HashSet<String> = HashSet::new();
    let mut stack = vec![decision_id.to_string()];
    let mut gap = false;
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) { continue; }
        for e in g.inn.get(&id).map(Vec::as_slice).unwrap_or(&[]) {
            let Some(s) = nodes.get(gs(e, "source")) else { continue };
            let t = gs(s, "type");
            if AGENT_TYPES.contains(&t) { continue; }
            if t == "decision" {
                if at(at(s, "decision"), "answeredBy").as_str().is_none_or(str::is_empty) { stack.push(gs(s, "id").to_string()); }
            } else { gap = true; }
        }
    }
    gap
}

/// handleLabel() (factory-model.js).
fn handle_label(n: &Value, h: &str) -> String {
    let find = |list: &[Value]| list.iter().find(|o| gs(o, "id") == h).map(|o| gs(o, "label")).filter(|l| !l.is_empty()).unwrap_or(h).to_string();
    match gs(n, "type") {
        "decision" => match h { "none" => "none of these".into(), "yes" | "no" => h.into(), _ => find(ga(at(n, "decision"), "options")) },
        "ask" => find(ga(at(n, "ask"), "replies")),
        "wait" => match h { "done" => "Done".into(), "timeout" => "Timed out".into(), _ => String::new() },
        _ => h.into(),
    }
}

// ─── Skill mentions (factory-skills.js) ───

/// findMentions(): skill names mentioned in a text, in order, without repeats
/// (the MENTION regex, by hand).
fn find_mentions(text: &str) -> Vec<String> {
    let cs: Vec<char> = text.chars().collect();
    let name_ch = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    // The name after a '/' at `p`: (end, ok).
    let name_at = |p: usize| -> Option<usize> {
        let mut j = p + 1;
        if !cs.get(j).is_some_and(|c| name_ch(*c)) { return None; }
        j += 1;
        while cs.get(j).is_some_and(|c| name_ch(*c) || *c == '-') { j += 1; }
        if cs.get(j) == Some(&':') && cs.get(j + 1).is_some_and(|c| name_ch(*c)) {
            j += 2;
            while cs.get(j).is_some_and(|c| name_ch(*c) || *c == '-') { j += 1; }
        }
        let next = cs.get(j).copied();
        let blocked = next.is_some_and(|c| is_word(c) || c == '/' || c == ':' || c == '-')
            || (next == Some('.') && cs.get(j + 1).is_some_and(|c| is_word(*c)));
        if blocked { None } else { Some(j) }
    };
    let mut out: Vec<String> = vec![];
    let mut p = 0;
    while p < cs.len() {
        let slash = if p == 0 && cs[0] == '/' { Some(0) }
            else if (is_ws(cs[p]) || "([{,;\"'`".contains(cs[p])) && cs.get(p + 1) == Some(&'/') { Some(p + 1) }
            else { None };
        match slash.and_then(|sl| name_at(sl).map(|end| (sl, end))) {
            Some((sl, end)) => {
                let name: String = cs[sl + 1..end].iter().collect();
                if !out.contains(&name) { out.push(name); }
                p = end;
            }
            None => p += 1,
        }
    }
    out
}

/// Where a skill is first named: (nodeId, employeeId, edgeId), "" when not set.
type At = (String, String, String);

/// mentionsInFactory(): every skill a normalized factory names, in first-seen
/// order, with where it is first named.
fn mentions_in_factory(f: &Value) -> Vec<(String, At)> {
    let mut out: Vec<(String, At)> = vec![];
    let mut add = |name: String, at: At| { if !out.iter().any(|(n, _)| *n == name) { out.push((name, at)); } };
    let none = String::new;
    for n in find_mentions(gs(f, "description")) { add(n, (none(), none(), none())); }
    for r in ga(f, "roster") {
        let id = gs(r, "id").to_string();
        for n in find_mentions(gs(r, "persona")) { add(n, (none(), id.clone(), none())); }
        for n in ga(r, "skills") {
            let s = js_string(n);
            add(s.strip_prefix('/').unwrap_or(&s).to_string(), (none(), id.clone(), none()));
        }
    }
    for node in ga(f, "nodes") {
        let id = gs(node, "id").to_string();
        match gs(node, "type") {
            "decision" => for n in find_mentions(gs(at(node, "decision"), "question")) { add(n, (id.clone(), none(), none())); },
            "ask" => for n in find_mentions(gs(at(node, "ask"), "prompt")) { add(n, (id.clone(), none(), none())); },
            _ => {}
        }
    }
    for e in ga(f, "edges") {
        for n in find_mentions(gs(e, "noteLabel")) { add(n, (none(), none(), gs(e, "id").to_string())); }
    }
    out
}

// ─── Hand-written approval rules ───

/// `/^[A-Za-z_][\w.-]*(\(.+\))?$/`
fn rule_shape(rule: &str) -> bool {
    let cs: Vec<char> = rule.chars().collect();
    if !cs.first().is_some_and(|c| c.is_ascii_alphabetic() || *c == '_') { return false; }
    let mut i = 1;
    while i < cs.len() && (is_word(cs[i]) || cs[i] == '.' || cs[i] == '-') { i += 1; }
    let rest = &cs[i..];
    rest.is_empty() || (rest.len() >= 3 && rest[0] == '(' && rest[rest.len() - 1] == ')' && !rest[1..rest.len() - 1].iter().any(|c| is_lt(*c)))
}

/// `rule === 'Bash' || /^Bash\(\s*\*?\s*:?\s*\*?\s*\)$/`
fn rule_too_broad(rule: &str) -> bool {
    if rule == "Bash" { return true; }
    let Some(inner) = rule.strip_prefix("Bash(").and_then(|r| r.strip_suffix(')')) else { return false };
    let mut it = inner.chars().peekable();
    for opt in ['*', ':', '*'] {
        while it.next_if(|c| is_ws(*c)).is_some() {}
        it.next_if_eq(&opt);
    }
    while it.next_if(|c| is_ws(*c)).is_some() {}
    it.next().is_none()
}

/// `/(^|[\s(])(sudo|su|doas)\b|git\s+push|git\s+remote/`
fn rule_forbidden(rule: &str) -> bool {
    let cs: Vec<char> = rule.chars().collect();
    let starts = |p: usize, w: &str| { let wc: Vec<char> = w.chars().collect(); cs.len() >= p + wc.len() && cs[p..p + wc.len()] == wc[..] };
    for p in 0..cs.len() {
        if p == 0 || is_ws(cs[p - 1]) || cs[p - 1] == '(' {
            for w in ["sudo", "su", "doas"] {
                if starts(p, w) && cs.get(p + w.len()).is_none_or(|c| !is_word(*c)) { return true; }
            }
        }
        if starts(p, "git") {
            let mut j = p + 3;
            while j < cs.len() && is_ws(cs[j]) { j += 1; }
            if j > p + 3 && (starts(j, "push") || starts(j, "remote")) { return true; }
        }
    }
    false
}

// ─── Validate ───

/// An issue: { code, message, fix, ...at }.
fn push(list: &mut Vec<Value>, code: &str, message: String, fix: String, at: Vec<(&str, Value)>) {
    let mut m = Map::new();
    m.insert("code".into(), json!(code));
    m.insert("message".into(), json!(message));
    m.insert("fix".into(), json!(fix));
    for (k, v) in at { m.insert(k.into(), v); }
    list.push(Value::Object(m));
}

/// `label(n)`: the node's label, or its type; "?" for no node.
fn label(n: Option<&&Value>) -> String {
    match n {
        None => "?".into(),
        Some(n) => { let l = gs(n, "label"); if l.is_empty() { gs(n, "type").into() } else { l.into() } }
    }
}

/// `"a", "b", "c"` for a cycle's nodes.
fn quoted_labels(c: &[String], nodes: &HashMap<&str, &Value>) -> String {
    c.iter().map(|id| format!("\"{}\"", label(nodes.get(id.as_str())))).collect::<Vec<_>>().join(", ")
}

/// validateFactory(f, { skills }): `{ ok, errors: [Issue], warnings: [Issue] }`,
/// Issue = `{ code, message, fix, nodeId?, nodeIds?, edgeId?, employeeId?, handle?, skill? }`.
/// ok means runnable. With `skills` (the names known on this machine), warns
/// about mentioned skills that are not among them.
pub(crate) fn validate(input: &Value, skills: Option<&HashSet<String>>) -> Value {
    let f = normalize(input);
    let mut errors: Vec<Value> = vec![];
    let mut warnings: Vec<Value> = vec![];
    let (errs, warns) = (&mut errors, &mut warnings);
    let id_v = |s: &str| json!(s);

    let node_list = ga(&f, "nodes");
    let roster_list = ga(&f, "roster");
    let edge_list = ga(&f, "edges");
    let nodes: HashMap<&str, &Value> = node_list.iter().map(|n| (gs(n, "id"), n)).collect();
    let roster: HashMap<&str, &Value> = roster_list.iter().map(|r| (gs(r, "id"), r)).collect();
    let lab = |n: &Value| label(Some(&n));

    // ── Roster ──
    if !roster.contains_key(MANAGER_ID) { push(errs, "roster-no-manager", "The roster has no Manager.".into(), "Add the Manager back to the roster.".into(), vec![]); }
    let mut names: HashSet<String> = HashSet::new();
    for r in roster_list {
        let (name, rid) = (gs(r, "name"), gs(r, "id"));
        if name.is_empty() {
            push(errs, "roster-unnamed", "An employee has no name.".into(), "Give every employee a name.".into(), vec![("employeeId", id_v(rid))]);
        } else {
            let k = name.to_lowercase();
            if names.contains(&k) { push(errs, "roster-duplicate-name", format!("Two employees are called \"{name}\"."), "Give each employee a different name.".into(), vec![("employeeId", id_v(rid))]); }
            names.insert(k);
        }
        if trim(gs(r, "persona")).is_empty() {
            push(warns, "roster-no-persona", format!("\"{}\" has no persona.", if name.is_empty() { "An employee" } else { name }), "Describe who this employee is and how they work.".into(), vec![("employeeId", id_v(rid))]);
        }
    }

    // ── Hand-written approval rules ──
    for rule in ga(at(&f, "settings"), "rules") {
        let rule = rule.as_str().unwrap_or("");
        let (shape, broad, forbidden) = (rule_shape(rule), rule_too_broad(rule), rule_forbidden(rule));
        if !shape || broad || forbidden {
            let why = if !shape { "is not a rule (write Tool or Tool(spec), e.g. Bash(npm test:*))" } else if broad { "allows every shell command, which is too broad" } else { "allows something that is never allowed" };
            push(warns, "rule-invalid", format!("The approval rule \"{rule}\" {why}; it will be ignored."), "Fix or remove it in the factory settings.".into(), vec![]);
        }
    }

    // ── Nodes ──
    let by_type = |t: &str| node_list.iter().filter(|n| gs(n, "type") == t).collect::<Vec<_>>();
    let (starts, managers, ends) = (by_type("start"), by_type("manager"), by_type("end"));
    let ids_of = |l: &[&Value]| Value::Array(l.iter().map(|n| id_v(gs(n, "id"))).collect());
    let many_at = |l: &[&Value]| if l.len() > 1 { vec![("nodeIds", ids_of(l))] } else { vec![] };
    if starts.len() != 1 {
        let some = !starts.is_empty();
        push(errs, if some { "start-many" } else { "start-missing" },
            if some { "There is more than one Start." } else { "There is no Start." }.into(),
            if some { "Keep one Start and delete the others." } else { "Add a Start node and connect it to the Manager." }.into(),
            many_at(&starts));
    }
    if managers.len() != 1 {
        let some = !managers.is_empty();
        push(errs, if some { "manager-many" } else { "manager-missing" },
            if some { "There is more than one Manager node." } else { "There is no Manager node." }.into(),
            if some { "A factory has exactly one Manager. Delete the extra ones." } else { "Add the Manager node." }.into(),
            many_at(&managers));
    }
    if ends.len() != 1 {
        let some = !ends.is_empty();
        push(errs, if some { "end-many" } else { "end-missing" },
            if some { "There is more than one End." } else { "There is no End." }.into(),
            if some { "Keep one End and delete the others." } else { "Add an End node and connect the Manager's Finish to it." }.into(),
            many_at(&ends));
    }

    let mut used: HashSet<&str> = HashSet::new();
    for n in node_list {
        let nid = || vec![("nodeId", id_v(gs(n, "id")))];
        let ty = gs(n, "type");
        if !NODE_TYPES.contains(&ty) { push(errs, "node-unknown-type", format!("\"{}\" is not a known kind of node.", lab(n)), "Delete it and add the node again.".into(), nid()); continue; }
        if ty == "employee" {
            let eid = gs(n, "employeeId");
            match roster.get(eid) {
                Some(r) if !eid.is_empty() => {
                    if gs(r, "id") == MANAGER_ID { push(errs, "employee-is-manager", format!("\"{}\" uses the Manager as an employee.", lab(n)), "Route to the Manager node instead, or pick another employee.".into(), nid()); }
                    else { used.insert(gs(r, "id")); }
                }
                _ => push(errs, "employee-unassigned", format!("\"{}\" has no employee assigned.", lab(n)), "Pick someone from the roster for this node.".into(), nid()),
            }
        }
        if ty == "decision" {
            let d = at(n, "decision");
            if trim(gs(d, "question")).is_empty() { push(errs, "decision-no-question", "A decision has no question.".into(), "Write the question this decision asks.".into(), nid()); }
            let kind = gs(d, "kind");
            if kind != "yesno" {
                let opts = ga(d, "options");
                if opts.len() < 2 { push(errs, "decision-few-options", format!("\"{}\" needs at least two options.", lab(n)), "Add another option.".into(), nid()); }
                for o in opts {
                    if gs(o, "label").is_empty() { push(errs, "decision-option-unlabelled", format!("An option of \"{}\" has no label.", lab(n)), "Label every option.".into(), nid()); }
                    if RESERVED_HANDLES.contains(&gs(o, "id")) { push(errs, "decision-option-reserved", format!("An option of \"{}\" uses a reserved id.", lab(n)), "Delete the option and add it again.".into(), nid()); }
                }
            }
            if at(d, "review").as_bool() == Some(true) && kind != "yesno" {
                push(warns, "review-not-yesno", format!("\"{}\" is marked as a review but is not a yes/no question, so the mark does nothing.", lab(n)), "Make it a yes/no question, or unmark it.".into(), nid());
            }
            let by = gs(d, "answeredBy");
            if !by.is_empty() && !roster.contains_key(by) {
                push(errs, "decision-answerer-missing", format!("\"{}\" is answered by someone no longer on the roster.", lab(n)), "Pick who answers this decision, or leave it to whoever arrives.".into(), nid());
            }
        }
        if ty == "wait" {
            let w = at(n, "wait");
            let command = gs(w, "kind") == "command";
            if command && trim(gs(w, "command")).is_empty() {
                push(errs, "wait-no-command", format!("\"{}\" waits for a command, but has none.", lab(n)), "Write the command to run, e.g. gh run watch --exit-status, or curl -fs localhost:4310/health.".into(), nid());
            }
            let num = |k: &str| at(w, k).as_f64().unwrap_or(0.0);
            if command && num("every") * 2.0 > num("timeout") * 60.0 {
                push(warns, "wait-few-tries", format!("\"{}\" gives up before it can try twice.", lab(n)), "Lengthen the time limit or check more often.".into(), nid());
            }
        }
        if ty == "ask" {
            let a = at(n, "ask");
            if trim(gs(a, "prompt")).is_empty() { push(errs, "ask-no-prompt", "An \"Ask me\" node has no question.".into(), "Write what you want to be asked.".into(), nid()); }
            let replies = ga(a, "replies");
            if replies.is_empty() { push(errs, "ask-no-replies", format!("\"{}\" has no replies to choose from.", lab(n)), "Add at least one reply.".into(), nid()); }
            for r in replies {
                if gs(r, "label").is_empty() { push(errs, "ask-reply-unlabelled", format!("A reply of \"{}\" has no label.", lab(n)), "Label every reply.".into(), nid()); }
                if RESERVED_HANDLES.contains(&gs(r, "id")) { push(errs, "ask-reply-reserved", format!("A reply of \"{}\" uses a reserved id.", lab(n)), "Delete the reply and add it again.".into(), nid()); }
            }
        }
    }
    for r in roster_list {
        let (rid, name) = (gs(r, "id"), gs(r, "name"));
        if rid != MANAGER_ID && !used.contains(rid) {
            push(warns, "roster-unused", format!("\"{}\" is on the roster but not in the flow.", if name.is_empty() { "An employee" } else { name }), "Place them on the canvas, or remove them from the roster.".into(), vec![("employeeId", id_v(rid))]);
        }
    }

    // ── Edges ──
    // "node|handle" -> edges, in first-use order.
    let mut handle_use: Vec<(String, &str, Vec<&Value>)> = vec![];
    for e in edge_list {
        let eid = || vec![("edgeId", id_v(gs(e, "id")))];
        let (Some(s), Some(t)) = (nodes.get(gs(e, "source")), nodes.get(gs(e, "target"))) else {
            push(errs, "edge-dangling", "A connection points at a node that no longer exists.".into(), "Delete the connection.".into(), eid());
            continue;
        };
        let (st, tt, handle) = (gs(s, "type"), gs(t, "type"), gs(e, "sourceHandle"));
        if gs(e, "kind") == "note" {
            if !AGENT_TYPES.contains(&st) || !AGENT_TYPES.contains(&tt) {
                push(errs, "note-not-agents", format!("A note from \"{}\" to \"{}\" must go between employees or the Manager.", lab(s), lab(t)), "Connect notes only between people.".into(), eid());
            }
            if gs(e, "source") == gs(e, "target") { push(errs, "note-self", format!("\"{}\" sends a note to itself.", lab(s)), "Delete the note.".into(), eid()); }
            continue;
        }
        if tt == "start" { push(errs, "edge-into-start", format!("\"{}\" connects back into Start.", lab(s)), "Route it to the Manager instead.".into(), eid()); }
        if st == "end" { push(errs, "edge-out-of-end", "End cannot lead anywhere.".into(), "Delete this connection.".into(), eid()); continue; }
        if !outputs_of(s).iter().any(|h| h == handle) {
            push(errs, "edge-bad-handle", format!("A connection leaves \"{}\" from an output it does not have.", lab(s)), "Delete it and connect from one of the node's outputs.".into(), eid());
            continue;
        }
        if st == "start" && tt != "manager" { push(errs, "start-not-manager", "Start must lead to the Manager.".into(), "Connect Start to the Manager node.".into(), eid()); }
        if handle == "finish" && tt != "end" { push(errs, "finish-not-end", "The Manager's Finish must lead to End.".into(), "Connect Finish to End.".into(), eid()); }
        if handle != "finish" && tt == "end" && st != "manager" {
            push(errs, "end-not-via-manager", format!("\"{}\" goes straight to End. The Manager always decides the end.", lab(s)), "Route it to the Manager; it finishes from there.".into(), eid());
        }
        if handle == "out" && st == "manager" && tt == "end" {
            push(errs, "end-not-via-finish", "The Manager reaches End through its Finish output only.".into(), "Delete this connection; Finish already goes to End.".into(), eid());
        }
        let k = format!("{}|{}", gs(e, "source"), handle);
        match handle_use.iter_mut().find(|(key, _, _)| *key == k) {
            Some((_, _, list)) => list.push(e),
            None => handle_use.push((k, gs(e, "source"), vec![e])),
        }
    }
    for (_, src, list) in &handle_use {
        if list.len() > 1 {
            let n = nodes.get(src);
            push(errs, "handle-many", format!("One output of \"{}\" has {} connections; only one is allowed.", label(n), list.len()), "To branch, add a Decision after this node.".into(),
                vec![("nodeId", id_v(src)), ("edgeId", id_v(gs(list[1], "id")))]);
        }
    }
    let wired: HashSet<&str> = handle_use.iter().map(|(k, _, _)| k.as_str()).collect();
    for n in node_list {
        let ty = gs(n, "type");
        for h in required_outputs_of(n) {
            if wired.contains(format!("{}|{}", gs(n, "id"), h).as_str()) { continue; }
            let what = if ty == "decision" { format!("the answer \"{}\"", handle_label(n, &h)) }
                else if ty == "ask" { format!("the reply \"{}\"", handle_label(n, &h)) }
                else if ty == "wait" && h != "out" { format!("its \"{}\" route", handle_label(n, &h)) }
                else if h == "finish" { "its Finish".into() }
                else { "its output".into() };
            push(errs, "output-unwired", format!("\"{}\" has nowhere to go for {what}.", lab(n)), "Connect it to the next step.".into(), vec![("nodeId", id_v(gs(n, "id"))), ("handle", json!(h))]);
        }
    }

    // ── Graph analysis (flow edges only; note edges never count) ──
    let g = flow_graph(&f);
    let known = |n: &Value| NODE_TYPES.contains(&gs(n, "type"));

    if let Some(start) = starts.first() {
        let from_start = reachable(&g, &[gs(start, "id")], false, None);
        for n in node_list {
            let nid = gs(n, "id");
            if from_start.has(nid) || !known(n) { continue; }
            let only_notes = edge_list.iter().any(|e| gs(e, "kind") == "note" && gs(e, "target") == nid);
            push(errs, if only_notes { "reached-only-by-notes" } else { "node-unreachable" },
                if only_notes { format!("\"{}\" is only reached by notes, so the flow never gets there.", lab(n)) } else { format!("\"{}\" can never be reached from Start.", lab(n)) },
                if only_notes { "Notes carry messages, not control. Connect a flow edge into it." } else { "Connect a step into it, or delete it." }.into(),
                vec![("nodeId", id_v(nid))]);
        }
    }

    if let Some(end) = ends.first() {
        let to_end = reachable(&g, &[gs(end, "id")], true, None);
        let mut stuck = IdSet::default();
        for n in node_list { if !to_end.has(gs(n, "id")) && known(n) { stuck.insert(gs(n, "id")); } }
        if !stuck.is_empty() {
            let loops: Vec<Vec<String>> = cycles(&g).into_iter().filter(|c| c.iter().all(|id| stuck.has(id))).collect();
            let in_loop: HashSet<&str> = loops.iter().flatten().map(String::as_str).collect();
            for c in &loops {
                push(errs, "loop-no-exit", format!("A loop through {} has no way out.", quoted_labels(c, &nodes)), "Add a decision answer inside the loop that leads out of it.".into(), vec![("nodeIds", json!(c))]);
            }
            for id in stuck.iter() {
                if in_loop.contains(id) { continue; }
                push(errs, "cannot-finish", format!("There is no way to finish from \"{}\".", label(nodes.get(id))), "Connect it onward so the flow can get back to the Manager.".into(), vec![("nodeId", id_v(id))]);
            }
        }
    }

    // Every cycle must pass through a limited edge: drop them and nothing may loop.
    let max_rounds = |e: &Value| at(e, "maxRounds").as_f64().unwrap_or(0.0);
    let unlimited = flow_graph_without(&f, |e| max_rounds(e) > 0.0);
    for c in cycles(&unlimited) {
        push(errs, "loop-no-limit", format!("The loop through {} has no round limit.", quoted_labels(&c, &nodes)), "Set a round limit on the connection that goes back round this loop.".into(), vec![("nodeIds", json!(c))]);
    }
    let in_some_cycle: HashSet<String> = cycles(&g).into_iter().flatten().collect();
    for e in edge_list {
        if gs(e, "kind") == "flow" && max_rounds(e) > 0.0
            && !(in_some_cycle.contains(gs(e, "source")) && in_some_cycle.contains(gs(e, "target")) && same_cycle(&g, e)) {
            push(warns, "limit-not-in-loop", "A round limit is set on a connection that is not part of a loop.".into(), "It has no effect; you can remove it.".into(), vec![("edgeId", id_v(gs(e, "id")))]);
        }
    }

    // ── Fan-out / Join ──
    let is_fork = |n: &Value| gs(n, "type") == "decision" && gs(at(n, "decision"), "kind") == "many";
    let forks: Vec<&Value> = node_list.iter().filter(|n| is_fork(n)).collect();
    let mut joins_by_fork: HashMap<&str, Vec<&Value>> = HashMap::new();
    for j in by_type("join") {
        let fork_id = gs(at(j, "join"), "forkId");
        match nodes.get(fork_id) {
            Some(fk) if !fork_id.is_empty() && is_fork(fk) => joins_by_fork.entry(gs(fk, "id")).or_default().push(j),
            _ => push(errs, "join-no-fork", format!("\"{}\" does not say which pick-many decision it joins.", lab(j)), "Choose the decision whose branches this Join waits for.".into(), vec![("nodeId", id_v(gs(j, "id")))]),
        }
    }
    let none: Vec<Value> = vec![];
    for fk in forks {
        let fid = gs(fk, "id");
        let js = joins_by_fork.get(fid).map(Vec::as_slice).unwrap_or(&[]);
        if js.len() != 1 {
            let some = !js.is_empty();
            push(errs, if some { "fork-many-joins" } else { "fork-no-join" },
                if some { format!("\"{}\" is closed by more than one Join.", lab(fk)) } else { format!("The branches of \"{}\" are never joined back together.", lab(fk)) },
                if some { "Keep one Join for this decision." } else { "Add a Join where its branches meet, and point it at this decision." }.into(),
                vec![("nodeId", id_v(fid))]);
            continue;
        }
        let j = js[0];
        let jid = gs(j, "id");
        let region = fork_region(&g, fid, jid);
        if region.has(fid) {
            push(errs, "fork-branch-loops-to-fork", format!("A branch of \"{}\" loops back to the decision before reaching \"{}\".", lab(fk), lab(j)), "Loop inside the branch, or go round after the Join.".into(), vec![("nodeId", id_v(fid))]);
        }
        for id in region.iter() {
            let Some(n) = nodes.get(id) else { continue };
            if matches!(gs(n, "type"), "end" | "manager" | "start") {
                push(errs, "fork-branch-escapes", format!("A branch of \"{}\" reaches \"{}\" before \"{}\".", lab(fk), lab(n), lab(j)), format!("Every branch must come back to \"{}\" first.", lab(j)), vec![("nodeId", id_v(fid))]);
                continue;
            }
            for e in g.inn.get(id).unwrap_or(&none) {
                let src = gs(e, "source");
                if src != fid && !region.has(src) {
                    push(errs, "fork-branch-entered", format!("\"{}\" sits inside a branch of \"{}\" but is also reached from outside it.", lab(n), lab(fk)), "Branches may only be entered from their decision.".into(), vec![("nodeId", id_v(id)), ("edgeId", id_v(gs(e, "id")))]);
                }
            }
        }
        for e in g.inn.get(jid).unwrap_or(&none) {
            let src = gs(e, "source");
            if !region.has(src) && src != fid {
                push(errs, "join-entered-outside", format!("\"{}\" is reached from \"{}\", which is not a branch of \"{}\".", lab(j), label(nodes.get(src)), lab(fk)), "Only branches of its decision may lead into a Join.".into(), vec![("nodeId", id_v(jid)), ("edgeId", id_v(gs(e, "id")))]);
            }
        }
        let fork_out = g.out.get(fid).unwrap_or(&none);
        if let Some(ne) = fork_out.iter().find(|e| gs(e, "sourceHandle") == "none") {
            if region.has(gs(ne, "target")) {
                push(errs, "fork-none-into-branch", format!("\"None of these\" on \"{}\" leads into one of its own branches.", lab(fk)), format!("Send it to \"{}\" or past it instead.", lab(j)), vec![("edgeId", id_v(gs(ne, "id")))]);
            }
        }
        for e in fork_out.iter().filter(|e| gs(e, "sourceHandle") != "none") {
            if gs(e, "target") == jid {
                push(warns, "fork-empty-branch", format!("The answer \"{}\" of \"{}\" goes straight to the Join and does nothing.", handle_label(fk, gs(e, "sourceHandle")), lab(fk)), "Route it to the employee who should do that work.".into(), vec![("edgeId", id_v(gs(e, "id")))]);
            }
        }
    }

    // ── Who answers each decision ──
    for n in node_list {
        if gs(n, "type") != "decision" || !gs(at(n, "decision"), "answeredBy").is_empty() { continue; }
        if answer_has_gap(&g, &nodes, gs(n, "id")) {
            push(errs, "decision-needs-answerer", format!("\"{}\" does not follow an employee, so nobody is there to answer it.", lab(n)), "Choose who answers this decision.".into(), vec![("nodeId", id_v(gs(n, "id")))]);
        }
    }

    if let Some(known_skills) = skills {
        for (name, (node_id, employee_id, edge_id)) in mentions_in_factory(&f) {
            if known_skills.contains(&name) { continue; }
            let mut where_at = vec![];
            if !node_id.is_empty() { where_at.push(("nodeId", json!(node_id))); }
            if !employee_id.is_empty() { where_at.push(("employeeId", json!(employee_id))); }
            if !edge_id.is_empty() { where_at.push(("edgeId", json!(edge_id))); }
            where_at.push(("skill", json!(name)));
            push(warns, "skill-unknown", format!("The skill /{name} is not installed for all projects."), "Install it, or make sure the projects you run this in have it.".into(), where_at);
        }
    }

    json!({ "ok": errors.is_empty(), "errors": errors, "warnings": warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skeleton() -> Value {
        json!({
            "name": "T",
            "roster": [{ "id": "manager", "name": "Manager", "persona": "p" }],
            "nodes": [
                { "id": "start", "type": "start", "label": "Task" },
                { "id": "manager", "type": "manager", "label": "Manager" },
                { "id": "end", "type": "end", "label": "Result" },
            ],
            "edges": [
                { "id": "e_start", "kind": "flow", "source": "start", "sourceHandle": "out", "target": "manager" },
                { "id": "e_finish", "kind": "flow", "source": "manager", "sourceHandle": "finish", "target": "end" },
            ],
        })
    }

    #[test]
    fn skeleton_is_valid() {
        let r = validate(&skeleton(), None);
        assert_eq!(r["ok"], json!(true), "{r}");
    }

    #[test]
    fn js_coercions() {
        assert_eq!(to_number(Some(&json!(" 0x1A "))), 26.0);
        assert!(to_number(Some(&json!("inf"))).is_nan());
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(num_str(1e21), "1e+21");
        assert_eq!(slug(Some(&json!("  Done when! "))), "done-when");
        assert_eq!(find_mentions("use /review and /a:b, not /home/x or a.js /x.y"), vec!["review", "a:b"]);
        assert!(rule_too_broad("Bash( * : * )") && !rule_too_broad("Bash(npm:*)"));
        assert!(rule_forbidden("Bash(sudo rm:*)") && !rule_forbidden("Bash(sudoku:*)") && rule_forbidden("Bash(git  push:*)"));
        assert!(rule_shape("Bash(npm test:*)") && !rule_shape("Bash()"));
    }

    #[test]
    fn unlimited_loop_is_reported() {
        let mut f = skeleton();
        f["edges"].as_array_mut().unwrap().push(json!({ "id": "e_loop", "source": "manager", "sourceHandle": "out", "target": "manager" }));
        let r = validate(&f, None);
        assert!(r["errors"].as_array().unwrap().iter().any(|e| e["code"] == "loop-no-limit"), "{r}");
    }
}
