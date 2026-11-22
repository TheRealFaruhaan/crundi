//! Factories: a run as a fold over its journal (src/factory-journal.js
//! reduce()), for the run graph's replay: the web folds the events with the
//! server's reducer to show any moment, its tokens (who was working, waiting
//! or asked), its history and its status as they were then.
//!
//! Only what the graph reads is kept: the engine's run (`state`) and, of the
//! runner's record, who was stopped for now (`retired`), the pause and the
//! cap (they set the status). Effects are not kept: they change nothing the
//! graph draws. The engine is engine.rs (the same pure rules the Dry run walks).

use super::engine;
use super::model::{arr, b, n, s, truthy};
use serde_json::{json, Value};

/// One event applied (reduce()); `prev` None: the empty record.
pub fn reduce(prev: Option<&Value>, ev: &Value) -> Value {
    let mut rec = prev.cloned().unwrap_or_else(|| json!({ "events": 0, "state": null, "run": { "retired": {}, "capExtra": 0, "capHit": null, "paused": null, "crashFrom": "" } }));
    rec["events"] = json!(n(&rec, "events") as i64 + 1);
    let at = s(ev, "at");
    let has_state = rec["state"].is_object();
    let cap = |rec: &Value| (n(&rec["state"]["snapshot"]["settings"]["caps"], "maxHandoffs") + n(&rec["run"], "capExtra")).max(0.0);
    // An engine call: its run when it applied (a refused one leaves the run as it was).
    let take = |rec: &mut Value, res: Value| -> bool {
        if !b(&res, "ok") { return false; }
        for e in arr(&res, "effects") { if s(e, "type") == "cap" { rec["run"]["capHit"] = json!({ "which": s(e, "which"), "limit": e["limit"], "at": at }); } }
        rec["state"] = res["run"].clone();
        true
    };
    match s(ev, "type").as_str() {
        "start" => {
            let mut st = engine::create_run(&s(ev, "id"), &s(ev, "project"), &ev["factory"], &s(ev, "task"));
            st["createdAt"] = json!(at);
            st["updatedAt"] = json!(at);
            rec["state"] = st;
        }
        "begin" if has_state => {
            let res = engine::start(&rec["state"]);
            if !take(&mut rec, res) { rec["state"]["status"] = json!("stopped"); }
        }
        "handoff" if has_state => {
            let c = cap(&rec);
            let res = engine::handoff_capped(&rec["state"], &s(ev, "tokenId"), &ev["payload"], &s(ev, "treeHash"), Some(c));
            if take(&mut rec, res) {
                // The skills of that step, its commit and who, on its history entry.
                if let Some(h) = rec["state"]["history"].as_array_mut().and_then(|h| h.iter_mut().rev().find(|x| matches!(s(x, "type").as_str(), "handoff" | "delegate-done" | "delegate-escalated"))) {
                    let skills = arr(ev, "skills");
                    if !skills.is_empty() { h["skills"] = json!(skills.iter().take(20).cloned().collect::<Vec<_>>()); }
                    if !s(ev, "commit").is_empty() { h["commit"] = json!(s(ev, "commit")); }
                    if s(h, "by").is_empty() && !s(ev, "by").is_empty() { h["by"] = json!(s(ev, "by")); }
                }
            }
        }
        "delegate" if has_state => {
            let c = cap(&rec);
            let res = engine::delegate(&rec["state"], &s(ev, "tokenId"), &s(ev, "employeeId"), &s(ev, "task"), Some(c));
            take(&mut rec, res);
        }
        "ask-reply" if has_state => { let res = engine::ask_reply(&rec["state"], &s(ev, "tokenId"), &s(ev, "replyId"), &s(ev, "note")); take(&mut rec, res); }
        "wait-done" if has_state => { let res = engine::wait_done(&rec["state"], &s(ev, "tokenId"), &s(ev, "outcome"), &s(ev, "detail")); take(&mut rec, res); }
        "escalate" if has_state => { let res = engine::escalate(&rec["state"], &s(ev, "tokenId"), &s(ev, "reason")); take(&mut rec, res); }
        "reopen" if has_state => {
            let as_message = ev.get("asMessage").map(|x| x != &json!(false)).unwrap_or(true);
            let res = engine::reopen(&rec["state"], &s(ev, "reason"), as_message);
            take(&mut rec, res);
        }
        "session" => { if let Some(r) = rec["run"]["retired"].as_object_mut() { r.remove(&s(ev, "employeeId")); } }
        "retire" => { rec["run"]["retired"][s(ev, "employeeId")] = json!(true); history(&mut rec, json!({ "type": "retire", "employeeId": s(ev, "employeeId") })); }
        "secret-used" => history(&mut rec, json!({ "type": "secret-used", "secret": s(ev, "secret"), "employee": s(ev, "employee"), "approvedBy": ev.get("approvedBy").cloned().unwrap_or(json!([])), "preauthorised": b(ev, "preauthorised") })),
        "trigger-message" => history(&mut rec, json!({ "type": "trigger-message", "name": s(ev, "name"), "title": s(ev, "title"), "key": s(ev, "key") })),
        "cap" => { rec["run"]["capHit"] = json!({ "which": s(ev, "which"), "limit": ev["limit"], "at": at }); }
        "pause" => {
            let by = { let x = s(ev, "by"); if x.is_empty() { "person".to_string() } else { x } };
            rec["run"]["paused"] = json!({ "by": by, "at": at });
        }
        "resume" if has_state => {
            if s(&rec["run"]["capHit"], "which") == "maxHandoffs" { let x = n(&rec["run"], "capExtra") + n(&rec["state"]["snapshot"]["settings"]["caps"], "maxHandoffs"); rec["run"]["capExtra"] = json!(x); }
            rec["run"]["capHit"] = Value::Null;
            rec["run"]["paused"] = Value::Null;
            rec["state"]["status"] = json!("running");
        }
        "merged" | "kept" | "discarded" if has_state => {
            let how = s(ev, "type");
            rec["state"]["status"] = json!("finished");
            let mut o = if rec["state"]["outcome"].is_object() { rec["state"]["outcome"].clone() } else { json!({}) };
            o["how"] = json!(how);
            if !s(ev, "commit").is_empty() { o["commit"] = json!(s(ev, "commit")); }
            rec["state"]["outcome"] = o;
        }
        "undone" if has_state => {
            let mut o = if rec["state"]["outcome"].is_object() { rec["state"]["outcome"].clone() } else { json!({}) };
            o["how"] = json!("undone");
            rec["state"]["outcome"] = o;
        }
        "stop" if has_state => { rec["state"]["status"] = json!("stopped"); }
        "crash" if has_state => {
            let from = if s(&rec["state"], "status") == "crashed" { let f = s(&rec["run"], "crashFrom"); if f.is_empty() { "running".into() } else { f } } else { s(&rec["state"], "status") };
            rec["run"]["crashFrom"] = json!(from);
            rec["state"]["status"] = json!("crashed");
        }
        "recover" if has_state => {
            let from = s(&rec["run"], "crashFrom");
            let st = if ["running", "waiting", "paused", "finishing"].contains(&from.as_str()) { from } else { "running".into() };
            if st != "paused" { rec["run"]["paused"] = Value::Null; }
            rec["state"]["status"] = json!(st);
            rec["run"]["crashFrom"] = json!("");
            history(&mut rec, json!({ "type": "recover", "reason": s(ev, "reason"), "note": s(ev, "note") }));
        }
        _ => {}
    }
    // applyHolds(): a pause or a cap wins over the engine's status.
    if rec["state"].is_object() {
        let st = s(&rec["state"], "status");
        if !["crashed", "stopped", "finished"].contains(&st.as_str()) {
            if truthy(&rec["run"]["paused"]) && st != "finishing" { rec["state"]["status"] = json!("paused"); }
            else if truthy(&rec["run"]["capHit"]) && st == "running" { rec["state"]["status"] = json!("waiting"); }
        }
        if !at.is_empty() { rec["state"]["updatedAt"] = json!(at); }
    }
    rec
}

fn history(rec: &mut Value, entry: Value) {
    if !rec["state"].is_object() { return; }
    let seq = n(&rec["state"], "seq") as i64 + 1;
    rec["state"]["seq"] = json!(seq);
    let mut e = json!({ "seq": seq });
    if let (Value::Object(a), Value::Object(b2)) = (&mut e, entry) { a.extend(b2); }
    if !rec["state"]["history"].is_array() { rec["state"]["history"] = json!([]); }
    if let Some(h) = rec["state"]["history"].as_array_mut() { h.push(e); }
}

/// asksOf(): the questions waiting on the person at that moment.
pub fn asks_of(rec: &Value) -> Vec<Value> {
    let st = &rec["state"];
    if !st.is_object() { return vec![]; }
    let nodes = arr(&st["snapshot"], "nodes");
    arr(st, "tokens").iter().filter(|t| s(t, "status") == "ask").map(|t| {
        let node = nodes.iter().find(|x| s(x, "id") == s(t, "nodeId"));
        json!({ "tokenId": s(t, "id"), "nodeId": s(t, "nodeId"), "prompt": node.map(|x| s(&x["ask"], "prompt")).unwrap_or_default(), "replies": node.map(|x| x["ask"]["replies"].clone()).unwrap_or(json!([])) })
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn factory() -> Value {
        json!({ "name": "F", "roster": [{ "id": "manager", "name": "Manager" }, { "id": "dev", "name": "Dev" }],
            "nodes": [
                { "id": "s", "type": "start", "position": { "x": 0, "y": 0 } },
                { "id": "m", "type": "manager", "position": { "x": 0, "y": 100 } },
                { "id": "d", "type": "employee", "employeeId": "dev", "position": { "x": 0, "y": 200 } },
                { "id": "a", "type": "ask", "ask": { "prompt": "Go on?", "replies": [{ "id": "yes", "label": "Yes" }] }, "position": { "x": 0, "y": 300 } },
            ],
            "edges": [
                { "id": "e1", "source": "s", "sourceHandle": "out", "target": "m", "kind": "flow" },
                { "id": "e2", "source": "m", "sourceHandle": "out", "target": "d", "kind": "flow" },
                { "id": "e3", "source": "d", "sourceHandle": "out", "target": "a", "kind": "flow" },
                { "id": "e4", "source": "a", "sourceHandle": "yes", "target": "m", "kind": "flow" },
            ] })
    }

    fn fold(evs: &[Value]) -> Value { let mut r: Option<Value> = None; for e in evs { r = Some(reduce(r.as_ref(), e)); } r.unwrap() }
    fn tokens(r: &Value) -> Vec<(String, String)> { arr(&r["state"], "tokens").iter().map(|t| (s(t, "nodeId"), s(t, "status"))).collect() }

    #[test]
    fn replays_who_is_where() {
        let start = json!({ "type": "start", "id": "r1", "project": "p", "factory": factory(), "task": "Do it", "at": "2026-01-01T00:00:00Z" });
        let begin = json!({ "type": "begin", "at": "2026-01-01T00:00:01Z" });
        let r = fold(&[start.clone(), begin.clone()]);
        assert_eq!(tokens(&r), vec![("m".to_string(), "work".to_string())]);
        assert_eq!(s(&r["state"], "status"), "running");
        let tid = s(&arr(&r["state"], "tokens")[0], "id");
        // The Manager hands off to Dev, Dev hands off to the Ask: the run waits on the person.
        let h1 = json!({ "type": "handoff", "tokenId": tid, "by": "manager", "payload": { "summary": "Plan" }, "at": "2026-01-01T00:01:00Z" });
        let r = fold(&[start.clone(), begin.clone(), h1.clone()]);
        assert_eq!(tokens(&r), vec![("d".to_string(), "work".to_string())]);
        let tid2 = s(&arr(&r["state"], "tokens")[0], "id");
        let h2 = json!({ "type": "handoff", "tokenId": tid2, "by": "dev", "payload": { "summary": "Built" }, "skills": ["x:build"], "at": "2026-01-01T00:02:00Z" });
        let r = fold(&[start.clone(), begin.clone(), h1.clone(), h2.clone()]);
        assert_eq!(tokens(&r), vec![("a".to_string(), "ask".to_string())]);
        assert_eq!(s(&r["state"], "status"), "waiting");
        assert_eq!(asks_of(&r)[0]["prompt"], json!("Go on?"));
        let last = arr(&r["state"], "history").iter().rev().find(|h| s(h, "type") == "handoff").cloned().unwrap();
        assert_eq!(last["skills"], json!(["x:build"]));
        // A pause holds it; a resume lets it go.
        let r = fold(&[start.clone(), begin.clone(), json!({ "type": "pause", "at": "x" })]);
        assert_eq!(s(&r["state"], "status"), "paused");
        let r = fold(&[start, begin, json!({ "type": "pause", "at": "x" }), json!({ "type": "resume", "at": "y" })]);
        assert_eq!(s(&r["state"], "status"), "running");
    }
}
