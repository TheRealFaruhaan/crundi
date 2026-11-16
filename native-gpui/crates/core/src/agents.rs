//! The agent dock (2.18, specs/agent-dock.md): a chat's sub-agents and
//! background commands, from its agent records: what a row, the panel and
//! the badges say, and the list's order. Pure.

use serde_json::Value;

fn st(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }

/// A command (a background task) or an agent.
pub fn is_task(a: &Value) -> bool { st(a, "kind") == "task" }

pub fn running(a: &Value) -> bool { let s = st(a, "status"); s.is_empty() || s == "running" }

pub fn shown(a: &Value) -> bool { !a.get("dismissed").and_then(|d| d.as_bool()).unwrap_or(false) }

/// agentLabel: a task's description, summary or "Background task"; an
/// agent's description, "<type> agent" or "Agent".
pub fn label(a: &Value) -> String {
    let d = st(a, "description");
    if !d.is_empty() { return d; }
    if is_task(a) { let s = st(a, "summary"); if s.is_empty() { "Background task".into() } else { s } }
    else { let t = st(a, "subagentType"); if t.is_empty() { "Agent".into() } else { format!("{t} agent") } }
}

fn tokens(a: &Value) -> Option<String> {
    let n = a.pointer("/usage/total_tokens").and_then(|x| x.as_f64())?;
    if n <= 0.0 { return None; }
    Some(if n >= 1000.0 { format!("{}k tok", (n / 1000.0).round() as u64) } else { format!("{} tok", n as u64) })
}

/// A list row's sub: type · progress / step / "running" (or "finished",
/// or the status) · tokens.
pub fn row_sub(a: &Value) -> String {
    let mut parts = vec![];
    let t = st(a, "subagentType");
    if !t.is_empty() { parts.push(t); }
    if running(a) {
        let p = st(a, "progress"); let s = st(a, "step");
        parts.push(if !p.is_empty() { p } else if !s.is_empty() { s } else { "running".into() });
    } else {
        let s = st(a, "status");
        parts.push(if s.is_empty() || s == "completed" { "finished".into() } else { s });
    }
    if let Some(k) = tokens(a) { parts.push(k); }
    parts.join(" · ")
}

/// The panel's sub: type · progress / step (running) or the status ("done")
/// · "<n> tool(s)" · "<n>k tok".
pub fn panel_sub(a: &Value) -> String {
    let mut parts = vec![];
    let t = st(a, "subagentType");
    if !t.is_empty() { parts.push(t); }
    let p = st(a, "progress"); let s = st(a, "step");
    if running(a) && (!p.is_empty() || !s.is_empty()) { parts.push(if !p.is_empty() { p } else { s }); }
    else { let x = st(a, "status"); parts.push(if x.is_empty() { "done".into() } else { x }); }
    if let Some(n) = a.pointer("/usage/tool_uses").and_then(|x| x.as_u64()) { parts.push(format!("{n} tool{}", if n == 1 { "" } else { "s" })); }
    if let Some(n) = a.pointer("/usage/total_tokens").and_then(|x| x.as_f64()).filter(|n| *n > 0.0) { parts.push(format!("{}k tok", (n / 1000.0).round() as u64)); }
    parts.join(" · ")
}

/// The list's order: running first; then finished, newest end first; ties
/// keep arrival order.
pub fn sorted(list: &[Value]) -> Vec<Value> {
    let mut v: Vec<(usize, &Value)> = list.iter().enumerate().collect();
    v.sort_by(|(i, a), (j, b)| {
        running(b).cmp(&running(a))
            .then_with(|| b.get("endedAt").and_then(|x| x.as_f64()).unwrap_or(0.0).partial_cmp(&a.get("endedAt").and_then(|x| x.as_f64()).unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal))
            .then(i.cmp(j))
    });
    v.into_iter().map(|(_, a)| a.clone()).collect()
}

/// A badge's tooltip: "3 background commands, 1 running — click to see
/// them"; "1 agent, all finished — click to see it".
pub fn tooltip(task: bool, n: usize, run: usize) -> String {
    let what = if task { if n == 1 { "background command" } else { "background commands" } } else if n == 1 { "agent" } else { "agents" };
    let state = if run == 0 { "all finished".to_string() } else { format!("{run} running") };
    format!("{n} {what}, {state} — click to see {}", if n == 1 { "it" } else { "them" })
}

/// The list's sub: "2 running · 2 finished", "1 finished".
pub fn list_sub(run: usize, done: usize) -> String {
    let mut p = vec![];
    if run > 0 { p.push(format!("{run} running")); }
    if done > 0 { p.push(format!("{done} finished")); }
    p.join(" · ")
}

/// What the panel says when it has no entries yet.
pub fn empty_text(a: &Value) -> &'static str {
    match (is_task(a), running(a)) {
        (false, true) => "Waiting for the agent’s first step…",
        (false, false) => "This agent’s transcript was not kept.",
        (true, true) => "Running in the background — it reports when something happens.",
        (true, false) => "This task has finished.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn words() {
        let a = json!({ "kind": "agent", "subagentType": "general-purpose", "status": "running", "progress": "Reading files", "usage": { "total_tokens": 21400, "tool_uses": 1 } });
        assert_eq!(label(&a), "general-purpose agent");
        assert_eq!(row_sub(&a), "general-purpose · Reading files · 21k tok");
        assert_eq!(panel_sub(&a), "general-purpose · Reading files · 1 tool · 21k tok");
        let t = json!({ "kind": "task", "status": "completed", "usage": { "total_tokens": 812 } });
        assert_eq!(label(&t), "Background task");
        assert_eq!(row_sub(&t), "finished · 812 tok");
        assert_eq!(panel_sub(&t), "completed · 1k tok");
        assert_eq!(row_sub(&json!({ "status": "failed" })), "failed");
        assert_eq!(tooltip(true, 3, 1), "3 background commands, 1 running — click to see them");
        assert_eq!(tooltip(false, 1, 0), "1 agent, all finished — click to see it");
        assert_eq!(list_sub(2, 2), "2 running · 2 finished");
        assert_eq!(list_sub(0, 1), "1 finished");
        assert_eq!(empty_text(&json!({ "kind": "task" })), "Running in the background — it reports when something happens.");
        assert!(!shown(&json!({ "dismissed": true })));
    }

    #[test]
    fn order() {
        let l = vec![json!({ "description": "old", "status": "completed", "endedAt": 1 }), json!({ "description": "run", "status": "running" }), json!({ "description": "new", "status": "completed", "endedAt": 5 })];
        let s: Vec<String> = sorted(&l).iter().map(label).collect();
        assert_eq!(s, vec!["run", "new", "old"]);
    }
}
