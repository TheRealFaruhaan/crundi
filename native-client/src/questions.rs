//! Questions (2.13, specs/questions.md): Claude's AskUserQuestion cards, as
//! the web's questionNode() (app/vendor/claude-chat.js). The questions, the
//! person's picks (one per question: the choices, Other, a note and the
//! choice it is shown under), what a pick does to the rest, when Submit is
//! possible, and what is sent. Pure: the app keeps the picks per card and
//! saves them as the card's answer draft.

use serde_json::{json, Map, Value};

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Opt {
    pub label: String,
    pub desc: String,
    pub preview: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Q {
    pub header: String,
    pub question: String,
    pub multi: bool,
    pub options: Vec<Opt>,
}

/// One question's picks (the web's `{chosen, other, note, noteOpt}`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Pick {
    /// The chosen labels, in the options' order.
    pub chosen: Vec<String>,
    pub other: String,
    pub note: String,
    /// The choice the note is shown under ("": the row is closed; the
    /// note's text is kept).
    pub note_opt: String,
}

fn st(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

/// The questions of an entry's `input`.
pub fn parse(input: &Value) -> Vec<Q> {
    input.get("questions").and_then(|q| q.as_array()).map(|qs| qs.iter().map(|q| Q {
        header: st(q, "header"),
        question: st(q, "question"),
        multi: q.get("multiSelect").and_then(|m| m.as_bool()).unwrap_or(false),
        options: q.get("options").and_then(|o| o.as_array()).map(|os| os.iter().map(|o| Opt { label: st(o, "label"), desc: st(o, "description"), preview: st(o, "preview") }).collect()).unwrap_or_default(),
    }).collect()).unwrap_or_default()
}

/// A card can be answered while pending, and when orphaned (its asking
/// process went: `cancelled`, or a restart: `unanswered`).
pub fn orphaned(status: &str) -> bool { status == "cancelled" || status == "unanswered" }
pub fn open(status: &str) -> bool { status == "pending" || orphaned(status) }

/// A note follows the selection: when its choice is no longer picked it
/// moves to the last picked one, or the row closes (the text is kept).
fn follow(p: &mut Pick) {
    if !p.note_opt.is_empty() && !p.chosen.contains(&p.note_opt) {
        p.note_opt = p.chosen.last().cloned().unwrap_or_default();
    }
}

/// A press on a choice: a single choice picks it (the others un-picked); a
/// multi-select toggles it. Either way Other is cleared (the web keeps
/// Other's text on a tick and ignores the ticks: a named difference).
pub fn pick(q: &Q, p: &mut Pick, label: &str) {
    if q.multi {
        let on = !p.chosen.iter().any(|c| c == label);
        let mut now: Vec<String> = q.options.iter().map(|o| o.label.clone())
            .filter(|l| if l == label { on } else { p.chosen.contains(l) }).collect();
        now.dedup();
        p.chosen = now;
    } else {
        p.chosen = vec![label.to_string()];
    }
    p.other.clear();
    follow(p);
}

/// The note button: picks the choice if it isn't, then the note shows
/// under it.
pub fn open_note(q: &Q, p: &mut Pick, label: &str) {
    if !p.chosen.iter().any(|c| c == label) { pick(q, p, label); }
    p.note_opt = label.to_string();
}

/// The note's ✕: no note, the row closed.
pub fn remove_note(p: &mut Pick) {
    p.note.clear();
    p.note_opt.clear();
}

/// Typing in Other: any text un-picks every choice of the question.
pub fn type_other(p: &mut Pick, text: &str) {
    p.other = text.to_string();
    if !text.is_empty() {
        p.chosen.clear();
        follow(p);
    }
}

/// A question's answer: Other's trimmed text, else the choices joined.
pub fn answer_of(p: &Pick) -> String {
    let o = p.other.trim();
    if !o.is_empty() { o.to_string() } else { p.chosen.join(", ") }
}

/// Submit is possible once every question has an answer.
pub fn can_submit(qs: &[Q], ps: &[Pick]) -> bool {
    !qs.is_empty() && qs.iter().enumerate().all(|(i, _)| ps.get(i).is_some_and(|p| !answer_of(p).is_empty()))
}

/// Nothing picked, typed or noted anywhere: no draft to keep.
pub fn blank(ps: &[Pick]) -> bool {
    ps.iter().all(|p| p.chosen.is_empty() && p.other.is_empty() && p.note.is_empty())
}

/// What `respond` sends as `updatedInput`: the question's own input with
/// `answers` (by question text) and, when a note was written, `annotations`.
pub fn updated_input(input: &Value, qs: &[Q], ps: &[Pick]) -> Value {
    let mut answers = Map::new();
    let mut notes = Map::new();
    for (i, q) in qs.iter().enumerate() {
        let p = ps.get(i).cloned().unwrap_or_default();
        answers.insert(q.question.clone(), json!(answer_of(&p)));
        let n = p.note.trim();
        if !n.is_empty() { notes.insert(q.question.clone(), json!({ "notes": n })); }
    }
    let mut out = if input.is_object() { input.clone() } else { json!({}) };
    out["answers"] = Value::Object(answers);
    if !notes.is_empty() { out["annotations"] = Value::Object(notes); }
    out
}

/// The answers alone (an orphaned card's `answer-closed`).
pub fn answers(qs: &[Q], ps: &[Pick]) -> Value {
    updated_input(&json!({}), qs, ps).get("answers").cloned().unwrap_or(json!({}))
}

/// The message an orphaned card's answer sends.
pub fn closed_text(qs: &[Q], ps: &[Pick]) -> String {
    let lines: Vec<String> = qs.iter().enumerate().map(|(i, q)| {
        let p = ps.get(i).cloned().unwrap_or_default();
        let n = p.note.trim();
        format!("{} -> {}{}", q.question, answer_of(&p), if n.is_empty() { String::new() } else { format!(" (note: {n})") })
    }).collect();
    format!("Answering the question from before the session restarted:\n{}", lines.join("\n"))
}

/// The picks as the draft keeps them (the web's `{v: picks}` items).
pub fn to_value(ps: &[Pick]) -> Value {
    Value::Array(ps.iter().map(|p| json!({ "chosen": p.chosen, "other": p.other, "note": p.note, "noteOpt": p.note_opt })).collect())
}

/// A saved draft back into picks, one per question; what no longer matches
/// the card (a choice it lacks) is dropped.
pub fn from_value(v: &Value, qs: &[Q]) -> Vec<Pick> {
    let arr = v.as_array().cloned().unwrap_or_default();
    qs.iter().enumerate().map(|(i, q)| {
        let x = arr.get(i).cloned().unwrap_or(Value::Null);
        let chosen: Vec<String> = x.get("chosen").and_then(|c| c.as_array()).map(|c| c.iter().filter_map(|l| l.as_str()).map(str::to_string).collect()).unwrap_or_default();
        let chosen: Vec<String> = q.options.iter().map(|o| o.label.clone()).filter(|l| chosen.contains(l)).collect();
        let chosen = if q.multi { chosen } else { chosen.into_iter().take(1).collect() };
        let mut p = Pick { chosen, other: st(&x, "other"), note: st(&x, "note"), note_opt: st(&x, "noteOpt") };
        if !q.options.iter().any(|o| o.label == p.note_opt) { p.note_opt.clear(); }
        follow(&mut p);
        p
    }).collect()
}

/// An answered card's look: `ok` tags (the answers, in question order),
/// the note lines, by Cru; or `dismissed`; or the neutral `answered`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Done {
    pub tags: Vec<String>,
    pub notes: Vec<String>,
    pub dismissed: bool,
    pub by_cru: bool,
}

pub fn done(e: &Value) -> Done {
    let ai = e.get("answeredInput").cloned().unwrap_or(Value::Null);
    let by_cru = st(e, "answeredBy") == "cru";
    if st(e, "status") == "denied" {
        return Done { dismissed: true, by_cru, ..Default::default() };
    }
    // In question order (JSON objects here are sorted by key): the card's
    // questions first, then any answer for a question it does not list.
    let order: Vec<String> = parse(e.get("input").unwrap_or(&ai)).into_iter().map(|q| q.question).collect();
    let in_order = |o: &Map<String, Value>| -> Vec<(String, Value)> {
        let mut keys: Vec<String> = order.iter().filter(|k| o.contains_key(*k)).cloned().collect();
        keys.extend(o.keys().filter(|k| !order.contains(k)).cloned());
        keys.into_iter().map(|k| (k.clone(), o[&k].clone())).collect()
    };
    let tags = ai.get("answers").and_then(|a| a.as_object()).map(|o| in_order(o).into_iter().map(|(_, v)| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).collect()).unwrap_or_default();
    let notes = ai.get("annotations").and_then(|a| a.as_object()).map(|o| in_order(o).into_iter().filter_map(|(_, v)| v.get("notes").and_then(|n| n.as_str()).map(str::to_string)).filter(|n| !n.is_empty()).map(|n| format!("Note: {n}")).collect()).unwrap_or_default();
    Done { tags, notes, dismissed: false, by_cru }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> Value {
        json!({ "questions": [
            { "header": "Database", "question": "Which database?", "multiSelect": false, "options": [
                { "label": "SQLite", "description": "a file" }, { "label": "MySQL", "description": "a server" }, { "label": "Postgres", "description": "x", "preview": "a\nb" } ] },
            { "question": "Which tests?", "multiSelect": true, "options": [
                { "label": "Unit", "description": "u" }, { "label": "Integration", "description": "i" }, { "label": "E2E", "description": "e" } ] }
        ] })
    }

    #[test]
    fn parses() {
        let qs = parse(&input());
        assert_eq!(qs.len(), 2);
        assert_eq!(qs[0].header, "Database");
        assert!(!qs[0].multi && qs[1].multi);
        assert_eq!(qs[0].options[2].preview, "a\nb");
        assert!(parse(&Value::Null).is_empty());
        assert!(orphaned("cancelled") && orphaned("unanswered") && !orphaned("pending"));
        assert!(open("pending") && open("unanswered") && !open("allowed"));
    }

    #[test]
    fn single_and_multi_picks_clear_other() {
        let qs = parse(&input());
        let mut p = Pick { other: "mine".into(), ..Default::default() };
        pick(&qs[0], &mut p, "MySQL");
        assert_eq!(p.chosen, vec!["MySQL"]);
        assert_eq!(p.other, "");
        pick(&qs[0], &mut p, "SQLite");
        assert_eq!(p.chosen, vec!["SQLite"]);
        let mut m = Pick { other: "x".into(), ..Default::default() };
        pick(&qs[1], &mut m, "E2E");
        pick(&qs[1], &mut m, "Unit");
        assert_eq!(m.chosen, vec!["Unit", "E2E"]);
        assert_eq!(m.other, "");
        pick(&qs[1], &mut m, "E2E");
        assert_eq!(m.chosen, vec!["Unit"]);
    }

    #[test]
    fn the_note_follows() {
        let qs = parse(&input());
        let mut p = Pick::default();
        open_note(&qs[0], &mut p, "SQLite");
        assert_eq!((p.chosen.clone(), p.note_opt.clone()), (vec!["SQLite".to_string()], "SQLite".to_string()));
        p.note = "keep it".into();
        pick(&qs[0], &mut p, "MySQL");
        assert_eq!(p.note_opt, "MySQL");
        assert_eq!(p.note, "keep it");
        type_other(&mut p, "neither");
        assert_eq!(p.note_opt, "");
        assert_eq!(p.note, "keep it");
        assert!(p.chosen.is_empty());
        remove_note(&mut p);
        assert_eq!(p.note, "");
        // Multi: un-ticking the noted choice moves the note to the last ticked.
        let mut m = Pick::default();
        pick(&qs[1], &mut m, "Unit");
        open_note(&qs[1], &mut m, "Integration");
        assert_eq!(m.chosen, vec!["Unit", "Integration"]);
        pick(&qs[1], &mut m, "Integration");
        assert_eq!(m.note_opt, "Unit");
    }

    #[test]
    fn answers_and_submit() {
        let qs = parse(&input());
        let mut ps = vec![Pick::default(), Pick::default()];
        assert!(!can_submit(&qs, &ps));
        pick(&qs[0], &mut ps[0], "MySQL");
        assert!(!can_submit(&qs, &ps));
        type_other(&mut ps[1], "  only JSON ");
        assert!(can_submit(&qs, &ps));
        type_other(&mut ps[1], "   ");
        assert!(!can_submit(&qs, &ps));
        type_other(&mut ps[1], "");
        pick(&qs[1], &mut ps[1], "Integration");
        pick(&qs[1], &mut ps[1], "Unit");
        assert_eq!(answer_of(&ps[1]), "Unit, Integration");
        ps[0].note = " under data/ ".into();
        let u = updated_input(&input(), &qs, &ps);
        assert_eq!(u["answers"], json!({ "Which database?": "MySQL", "Which tests?": "Unit, Integration" }));
        assert_eq!(u["annotations"], json!({ "Which database?": { "notes": "under data/" } }));
        assert_eq!(u["questions"], input()["questions"]);
        ps[0].note.clear();
        assert!(updated_input(&input(), &qs, &ps).get("annotations").is_none());
        ps[0].note = "n".into();
        assert_eq!(closed_text(&qs, &ps), "Answering the question from before the session restarted:\nWhich database? -> MySQL (note: n)\nWhich tests? -> Unit, Integration");
        assert_eq!(answers(&qs, &ps), json!({ "Which database?": "MySQL", "Which tests?": "Unit, Integration" }));
    }

    #[test]
    fn drafts_round_trip() {
        let qs = parse(&input());
        let ps = vec![Pick { chosen: vec!["MySQL".into()], other: String::new(), note: "n".into(), note_opt: "MySQL".into() }, Pick { chosen: vec!["Unit".into(), "E2E".into()], ..Default::default() }];
        assert_eq!(from_value(&to_value(&ps), &qs), ps);
        assert!(blank(&[Pick::default()]));
        assert!(!blank(&ps));
        // A stale choice is dropped; a single question keeps one.
        let v = json!([{ "chosen": ["Gone", "SQLite", "MySQL"], "noteOpt": "Gone", "note": "x" }]);
        let back = from_value(&v, &qs);
        assert_eq!(back[0].chosen, vec!["SQLite"]);
        assert_eq!(back[0].note_opt, "");
        assert_eq!(back[1], Pick::default());
    }

    #[test]
    fn answered_looks() {
        let e = json!({ "status": "allowed", "answeredBy": "cru", "input": { "questions": [{ "question": "B?" }, { "question": "A?" }] }, "answeredInput": { "answers": { "A?": "x", "B?": "y, z" }, "annotations": { "A?": { "notes": "n" } } } });
        assert_eq!(done(&e), Done { tags: vec!["y, z".into(), "x".into()], notes: vec!["Note: n".into()], dismissed: false, by_cru: true });
        assert!(done(&json!({ "status": "denied" })).dismissed);
        assert_eq!(done(&json!({ "status": "answered-late", "answeredInput": { "answers": {} } })), Done::default());
    }
}
