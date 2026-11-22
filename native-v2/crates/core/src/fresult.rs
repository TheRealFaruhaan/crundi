//! A factory's result card (2.20, specs/cards.md): what the card shows for
//! its entry (`kind: 'factory-result'`), as the web's factoryResultNode()
//! (app/vendor/claude-chat.js): the counts, the files, the state's message,
//! the usage line, the follow-ups, the buttons per state, and the body of
//! a choice. Pure.

use serde_json::{json, Value};

fn st(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }
fn num(v: &Value, k: &str) -> f64 { v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0) }

/// One button: its choice (merge | changes | keep | discard | undo), label, look.
#[derive(Clone, Debug, PartialEq)]
pub struct Btn { pub key: &'static str, pub label: String, pub primary: bool, pub danger: bool }

#[derive(Clone, Debug, PartialEq)]
pub struct Fu { pub id: String, pub text: String, pub place: String, pub blocking: bool }

/// Markdown's own characters taken literally (a branch name, a factory's).
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\`*_[]<>#!|~".contains(c) { o.push('\\'); }
        o.push(c);
    }
    o
}

pub fn title(e: &Value) -> String {
    let f = st(e, "factory");
    if f.is_empty() { "Factory result".into() } else { format!("Factory result · {f}") }
}

/// The files line's parts: "+A", "−R", "in N file(s)", the branch.
pub fn counts(e: &Value) -> (String, String, String) {
    let d = e.get("diff").cloned().unwrap_or(Value::Null);
    let n = num(&d, "count") as u64;
    (format!("+{}", num(&d, "added") as u64), format!("−{}", num(&d, "removed") as u64), format!("in {n} file{}", if n == 1 { "" } else { "s" }))
}

/// The file rows (name, "+a", "−r"; "?" for null) and "and N more".
pub fn files(e: &Value) -> (Vec<(String, String, String)>, String) {
    let d = e.get("diff").cloned().unwrap_or(Value::Null);
    let list = d.get("files").and_then(|f| f.as_array()).cloned().unwrap_or_default();
    let n = |f: &Value, k: &str| f.get(k).and_then(|x| x.as_f64()).map(|x| (x as u64).to_string()).unwrap_or_else(|| "?".into());
    let rows: Vec<(String, String, String)> = list.iter().map(|f| (st(f, "file"), format!("+{}", n(f, "added")), format!("−{}", n(f, "removed")))).collect();
    let count = num(&d, "count") as usize;
    let more = if count > rows.len() { format!("and {} more", count - rows.len()) } else { String::new() };
    (rows, more)
}

/// Show the full diff: only while the work is there to look at.
pub fn can_diff(e: &Value) -> bool { matches!(st(e, "state").as_str(), "ask" | "conflict" | "changes") }

/// The state's message, as markdown (bold parts, the commit as code).
pub fn message(e: &Value) -> String {
    let parent = { let p = st(e, "parentBranch"); if p.is_empty() { "the parent branch".to_string() } else { p } };
    let (p, b) = (esc(&parent), esc(&st(e, "branch")));
    let count = e.pointer("/diff/count").and_then(|x| x.as_f64()).unwrap_or(0.0);
    match st(e, "state").as_str() {
        "merged" => {
            let commit: String = st(e, "commit").chars().take(7).collect();
            let auto = e.get("auto").and_then(|x| x.as_bool()).unwrap_or(false);
            format!("{} into **{p}** as one commit{}.", if auto { "Merged automatically" } else { "Merged" }, if commit.is_empty() { String::new() } else { format!(" `{commit}`") })
        }
        "undone" => format!("Undone. {p} is back where it was; the work is kept on **{b}**."),
        "kept" => format!("Kept on the branch **{b}**. Nothing was merged."),
        "discarded" => "Discarded. The worktree and branch are gone.".into(),
        "conflict" => format!("Merging into {p} conflicts. The Manager is bringing {p} in and resolving it, then it will finish again."),
        "changes" => "Changes requested. The team is working on them; this card updates when the Manager finishes again.".into(),
        "ask" if count == 0.0 => "The team made no changes, so there is nothing to merge. Ask for changes, or close the run.".into(),
        "ask" => "Look it over, then choose what happens to it.".into(),
        _ => String::new(),
    }
}

/// "$0.18 · 1 turn · 1 handoff · 1 chat · 2 h 25 min" (pluralised: a named
/// difference); "" when no turns and no cost.
pub fn usage(e: &Value) -> String {
    let Some(u) = e.get("usage").filter(|u| u.is_object()) else { return String::new() };
    let (cost, turns) = (num(u, "costUsd"), num(u, "turns") as u64);
    if turns == 0 && cost == 0.0 { return String::new(); }
    let pl = |n: u64, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
    let mut s = if cost != 0.0 { format!("${cost:.2} · ") } else { String::new() };
    s += &format!("{} · {} · {}", pl(turns, "turn"), pl(num(u, "handoffs") as u64, "handoff"), pl(num(u, "chats") as u64, "chat"));
    let mins = (num(u, "activeMs") / 60000.0).round() as u64;
    if mins > 0 { s += &if mins >= 60 { format!(" · {} h {} min", mins / 60, mins % 60) } else { format!(" · {mins} min") }; }
    s
}

pub fn followups(e: &Value) -> Vec<Fu> {
    e.get("followUps").and_then(|f| f.as_array()).map(|a| a.iter().map(|x| Fu {
        id: st(x, "id"), text: st(x, "text"), place: st(x, "where"),
        blocking: x.get("blocking").and_then(|b| b.as_bool()).unwrap_or(false),
    }).collect()).unwrap_or_default()
}

pub fn followups_line(n: usize) -> String { format!("{n} follow-up{} from review", if n == 1 { "" } else { "s" }) }

/// The buttons for the card's state; Discard reads "Sure? Discard" while armed.
pub fn buttons(e: &Value, armed: bool) -> Vec<Btn> {
    let parent = { let p = st(e, "parentBranch"); if p.is_empty() { "the parent branch".to_string() } else { p } };
    let b = |key, label: &str, primary, danger| Btn { key, label: label.into(), primary, danger };
    match st(e, "state").as_str() {
        "ask" if e.pointer("/diff/count").and_then(|x| x.as_f64()).unwrap_or(0.0) == 0.0 => vec![b("changes", "Request changes", true, false), b("close", "Close the run", false, true)],
        "ask" => vec![
            b("merge", &format!("Merge into {parent}"), true, false),
            b("changes", "Request changes", false, false),
            b("keep", "Keep branch", false, false),
            b("discard", if armed { "Sure? Discard" } else { "Discard" }, false, true),
        ],
        "merged" => vec![b("undo", "Undo", false, false)],
        _ => vec![],
    }
}

/// `POST /api/factory-runs`'s body for a choice.
pub fn finish_body(run_id: &str, choice: &str, note: Option<&str>) -> Value {
    let mut b = json!({ "action": "finish", "id": run_id, "choice": choice });
    if let Some(n) = note { b["note"] = json!(n); }
    b
}

/// The full diff's text as lines (with "… (cut short)" when cut), and the
/// longest line's length in characters (the view's width).
pub fn diff_lines(v: &Value) -> Result<(Vec<String>, usize), String> {
    if v.get("ok").and_then(|o| o.as_bool()) != Some(true) {
        let e = st(v, "error");
        return Err(if e.is_empty() { "Could not load the diff".into() } else { e });
    }
    let mut text = st(v, "diff");
    if text.is_empty() { text = "(no changes)".into(); }
    if v.get("truncated").and_then(|t| t.as_bool()).unwrap_or(false) { text += "\n… (cut short)"; }
    let lines: Vec<String> = text.split('\n').map(|l| crate::tools::expand_tabs(l.trim_end_matches('\r'))).collect();
    let w = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    Ok((lines, w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(state: &str) -> Value {
        json!({ "id": "fr1", "kind": "factory-result", "runId": "r1", "state": state, "factory": "Site builder",
            "summary": "Built it.", "branch": "factory/site_builder", "parentBranch": "dev", "commit": "6b00fea123", "auto": false,
            "diff": { "added": 130, "removed": 33, "count": 42, "files": [{ "file": "a.js", "added": 3, "removed": 1 }, { "file": "logo.png", "added": null, "removed": null }] },
            "followUps": [{ "id": "F1", "text": "Tidy", "where": "a.js", "blocking": false }, { "id": "F3", "text": "Broken", "blocking": true }],
            "usage": { "costUsd": 0.18, "turns": 1, "handoffs": 1, "chats": 1, "activeMs": 0 } })
    }

    #[test]
    fn parts() {
        let e = card("ask");
        assert_eq!(title(&e), "Factory result · Site builder");
        assert_eq!(counts(&e), ("+130".into(), "−33".into(), "in 42 files".into()));
        let (rows, more) = files(&e);
        assert_eq!(rows[1], ("logo.png".into(), "+?".into(), "−?".into()));
        assert_eq!(more, "and 40 more");
        assert!(can_diff(&e) && !can_diff(&card("merged")));
        assert_eq!(usage(&e), "$0.18 · 1 turn · 1 handoff · 1 chat");
        let mut long = card("ask");
        long["usage"] = json!({ "costUsd": 4.37, "turns": 58, "handoffs": 14, "chats": 5, "activeMs": 8_700_000 });
        assert_eq!(usage(&long), "$4.37 · 58 turns · 14 handoffs · 5 chats · 2 h 25 min");
        long["usage"] = json!({ "turns": 3, "handoffs": 2, "chats": 1, "activeMs": 20_000 });
        assert_eq!(usage(&long), "3 turns · 2 handoffs · 1 chat");
        long["usage"] = json!({ "turns": 0 });
        assert_eq!(usage(&long), "");
        assert_eq!(followups(&e)[1], Fu { id: "F3".into(), text: "Broken".into(), place: "".into(), blocking: true });
        assert_eq!(followups_line(1), "1 follow-up from review");
    }

    #[test]
    fn messages() {
        assert_eq!(message(&card("ask")), "Look it over, then choose what happens to it.");
        assert_eq!(message(&card("merged")), "Merged into **dev** as one commit `6b00fea`.");
        assert_eq!(message(&card("kept")), "Kept on the branch **factory/site\\_builder**. Nothing was merged.");
        let mut e = card("ask");
        e["diff"]["count"] = json!(0);
        assert!(message(&e).starts_with("The team made no changes"));
        assert_eq!(buttons(&e, false).iter().map(|b| b.label.as_str()).collect::<Vec<_>>(), ["Request changes", "Close the run"]);
        e["parentBranch"] = json!("");
        e["state"] = json!("conflict");
        assert!(message(&e).starts_with("Merging into the parent branch conflicts."));
        let mut m = card("merged");
        m["auto"] = json!(true);
        m["commit"] = json!("");
        assert_eq!(message(&m), "Merged automatically into **dev** as one commit.");
    }

    #[test]
    fn buttons_and_bodies() {
        let b = buttons(&card("ask"), false);
        assert_eq!(b.iter().map(|b| b.label.as_str()).collect::<Vec<_>>(), ["Merge into dev", "Request changes", "Keep branch", "Discard"]);
        assert!(b[0].primary && b[3].danger);
        assert_eq!(buttons(&card("ask"), true)[3].label, "Sure? Discard");
        assert_eq!(buttons(&card("merged"), false)[0].key, "undo");
        assert!(buttons(&card("changes"), false).is_empty());
        assert_eq!(finish_body("r1", "changes", Some("More tests")), json!({ "action": "finish", "id": "r1", "choice": "changes", "note": "More tests" }));
        let (l, w) = diff_lines(&json!({ "ok": true, "diff": "diff --git a b\n+x\tyz", "truncated": true })).unwrap();
        assert_eq!(l, ["diff --git a b", "+x      yz", "… (cut short)"]);
        assert_eq!(w, 14);
        assert_eq!(diff_lines(&json!({ "ok": false })).unwrap_err(), "Could not load the diff");
    }
}
