//! The log's rows (ported from the Slint client's app.rs `rows_of` and
//! `split_blocks`, unchanged): what one row of a chat log shows. Claude's
//! text in blocks (paragraphs, list items with their markers, headings,
//! quotes, rules, fenced code); a call that started a sub-agent is an agent
//! row (never folded); calls Claude Code summed up fold behind one group head
//! drawn where the group's first call is; any other call is a card; thinking
//! blocks; side questions after the entry that was last when they were asked.
//! Pure: the views draw these (one module per kind, AREAS.md).

use crundi_core::{chat::Shown, md, think::Think};
use serde_json::Value;
use std::collections::HashSet;

/// A side question (2.21, aside.rs): asked while a turn ran, answered aside.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Aside {
    pub id: u32,
    /// The entry that was last when it was asked ("" for an empty log).
    pub anchor: String,
    pub question: String,
    /// "asking" | "done" | "error".
    pub state: String,
    pub text: String,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Row {
    /// Anything else: user, result, error, notice, permission, question.
    Entry(Shown),
    /// Claude's text in blocks: (id, markdown, kind, marker, last). Kind:
    /// "p" | "h" | "h4" | "hr" | "quote" | "li" | "lie" (a list's last item).
    /// Last: the message's last block (the gap after it is an entry's).
    Prose(String, String, &'static str, String, bool),
    /// A fenced code block: (id, code, last).
    Code(String, String, bool),
    /// One tool card (2.14): (id, the call, open?).
    Tool(String, Shown, bool),
    /// A thinking block (2.15): (the entry, open?).
    Think(Shown, bool),
    /// A side question's card (2.21).
    Aside(Aside),
    /// A batch Claude Code summed up: (group id "g:<group>", its summary,
    /// the members with their open state, the group open?).
    Tools(String, String, Vec<(Shown, bool)>, bool),
    /// A call that started a sub-agent (2.18): (id, its agent record).
    Agent(String, Value),
}

fn s(v: &Value, k: &str) -> String {
    match v.get(k) { Some(Value::String(x)) => x.clone(), Some(Value::Null) | None => String::new(), Some(o) => o.to_string() }
}

/// Claude's text as blocks (ids "<entry>#<n>").
pub fn split_blocks(id: &str, text: &str) -> Vec<Row> {
    use md::Line;
    let mut out: Vec<Row> = vec![];
    let mut para: Vec<String> = vec![];
    let mut code: Option<String> = None;
    // A numbered list counts from 1 (the web's <ol>), until it ends.
    let mut num = 0;
    let n = |out: &Vec<Row>| format!("{id}#{}", out.len());
    let flush = |para: &mut Vec<String>, out: &mut Vec<Row>| {
        if !para.is_empty() {
            let refs: Vec<&str> = para.iter().map(String::as_str).collect();
            out.push(Row::Prose(n(out), md::paragraph(&refs), "p", String::new(), false));
        }
        para.clear();
    };
    for raw in text.split('\n') {
        if let Some(c) = &mut code {
            if md::line(raw) == Line::Fence {
                // One trailing newline dropped.
                let body = c.strip_suffix('\n').unwrap_or(c).to_string();
                code = None;
                out.push(Row::Code(n(&out), body, false));
            } else { c.push_str(raw); c.push('\n'); }
            continue;
        }
        let l = md::line(raw);
        if !matches!(l, Line::Numbered(_)) { num = 0; }
        match l {
            Line::Fence => { flush(&mut para, &mut out); code = Some(String::new()); }
            Line::Blank => flush(&mut para, &mut out),
            Line::Heading(top, h) => { flush(&mut para, &mut out); out.push(Row::Prose(n(&out), format!("**{}**", md::linkify(h)), if top { "h" } else { "h4" }, String::new(), false)); }
            Line::Rule => { flush(&mut para, &mut out); out.push(Row::Prose(n(&out), String::new(), "hr", String::new(), false)); }
            Line::Quote(q) => { flush(&mut para, &mut out); out.push(Row::Prose(n(&out), md::linkify(q), "quote", String::new(), false)); }
            Line::Bullet(r) => { flush(&mut para, &mut out); out.push(Row::Prose(n(&out), md::linkify(r), "li", "•".into(), false)); }
            Line::Numbered(r) => { flush(&mut para, &mut out); num += 1; out.push(Row::Prose(n(&out), md::linkify(r), "li", format!("{num}."), false)); }
            Line::Text(t) => para.push(t.to_string()),
        }
    }
    if let Some(c) = code { out.push(Row::Code(n(&out), c.strip_suffix('\n').unwrap_or(&c).to_string(), false)); }
    flush(&mut para, &mut out);
    // A list's last item: the list's gap below it. A numbered list and a
    // bulleted one are two lists, as the web's <ol> and <ul>.
    for k in 0..out.len() {
        let bullet = |m: &str| m == "•";
        let next_same = match (out.get(k), out.get(k + 1)) {
            (Some(Row::Prose(_, _, "li", m, _)), Some(Row::Prose(_, _, "li", n, _))) => bullet(m) == bullet(n),
            _ => false,
        };
        if let Row::Prose(_, _, kind, _, _) = &mut out[k] { if *kind == "li" && !next_same { *kind = "lie"; } }
    }
    match out.last_mut() {
        Some(Row::Prose(_, _, _, _, last)) | Some(Row::Code(_, _, last)) => *last = true,
        _ => {}
    }
    out
}

/// The log's rows from the conversation's entries, its agents, the cards the
/// reader opened, the thinking rules and the side questions.
pub fn rows_of(shown: &[Shown], agents: &[Value], open: &HashSet<String>, think: &Think, asides: &[Aside]) -> Vec<Row> {
    let agent_of = |e: &Shown| -> Option<&Value> {
        if e.tool_use_id.is_empty() { return None; }
        agents.iter().find(|a| s(a, "toolUseId") == e.tool_use_id && s(a, "kind") != "task")
    };
    let grouped = |e: &Shown| e.kind == "tool" && !e.group.is_empty() && !e.group_summary.is_empty() && agent_of(e).is_none();
    let mut groups: Vec<(String, Vec<&Shown>)> = vec![];
    for e in shown.iter().filter(|e| grouped(e)) {
        match groups.iter_mut().find(|g| g.0 == e.group) { Some(g) => g.1.push(e), None => groups.push((e.group.clone(), vec![e])) }
    }
    let mut drawn: HashSet<String> = HashSet::new();
    let mut rows = vec![];
    let ids: HashSet<&str> = shown.iter().map(|e| e.id.as_str()).collect();
    for x in asides.iter().filter(|x| x.anchor.is_empty()) { rows.push(Row::Aside(x.clone())); }
    for e in shown {
        let after: Vec<Row> = asides.iter().filter(|x| x.anchor == e.id).map(|x| Row::Aside(x.clone())).collect();
        if e.kind == "tool" {
            if let Some(a) = agent_of(e) {
                rows.push(Row::Agent(e.id.clone(), a.clone()));
            } else if grouped(e) {
                if !drawn.insert(e.group.clone()) { continue; }
                let members = groups.iter().find(|g| g.0 == e.group).map(|g| g.1.clone()).unwrap_or_default();
                let gid = format!("g:{}", e.group);
                rows.push(Row::Tools(gid.clone(), e.group_summary.clone(), members.into_iter().map(|m| (m.clone(), open.contains(&m.id))).collect(), open.contains(&gid)));
            } else {
                rows.push(Row::Tool(e.id.clone(), e.clone(), open.contains(&e.id)));
            }
        } else if e.kind == "assistant" {
            rows.extend(split_blocks(&e.id, &e.text));
        } else if e.kind == "thinking" {
            rows.push(Row::Think(e.clone(), think.open(&e.id)));
        } else {
            rows.push(Row::Entry(e.clone()));
        }
        rows.extend(after);
    }
    for x in asides.iter().filter(|x| !x.anchor.is_empty() && !ids.contains(x.anchor.as_str())) { rows.push(Row::Aside(x.clone())); }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(rows: &[Row]) -> Vec<String> {
        rows.iter().map(|r| match r {
            Row::Prose(_, _, k, m, last) => format!("{k}{m}{}", if *last { "!" } else { "" }),
            Row::Code(_, c, last) => format!("code:{c}{}", if *last { "!" } else { "" }),
            _ => "other".into(),
        }).collect()
    }

    #[test]
    fn blocks() {
        let rows = split_blocks("a", "# Title\nsome text\nmore\n\n- one\n- two\n1. x\n2. y\n\n```\nlet a = 1;\n```");
        assert_eq!(kinds(&rows), vec!["h", "p", "li•", "lie•", "li1.", "lie2.", "code:let a = 1;!"]);
        if let Row::Prose(id, t, ..) = &rows[1] { assert_eq!(id, "a#1"); assert!(t.contains("some text")); }
    }

    #[test]
    fn an_unclosed_fence_is_code_to_the_end() {
        let rows = split_blocks("a", "intro\n```\nfn x() {}");
        assert_eq!(kinds(&rows), vec!["p", "code:fn x() {}!"]);
    }

    #[test]
    fn groups_fold_and_asides_follow_their_anchor() {
        let tool = |id: &str, g: &str| Shown { id: id.into(), kind: "tool", group: g.into(), group_summary: if g.is_empty() { String::new() } else { "Read 2 files".into() }, ..Default::default() };
        let shown = vec![tool("t1", "G"), tool("t2", "G"), tool("t3", ""), Shown { id: "u".into(), kind: "user", ..Default::default() }];
        let asides = vec![Aside { id: 1, anchor: "t3".into(), ..Default::default() }, Aside { id: 2, anchor: "gone".into(), ..Default::default() }];
        let mut open = HashSet::new();
        open.insert("t2".to_string());
        let rows = rows_of(&shown, &[], &open, &Think::default(), &asides);
        assert!(matches!(&rows[0], Row::Tools(g, s, ms, false) if g == "g:G" && s == "Read 2 files" && ms.len() == 2 && ms[1].1));
        assert!(matches!(&rows[1], Row::Tool(id, _, false) if id == "t3"));
        assert!(matches!(&rows[2], Row::Aside(a) if a.id == 1));
        assert!(matches!(&rows[3], Row::Entry(e) if e.id == "u"));
        assert!(matches!(&rows[4], Row::Aside(a) if a.id == 2));
    }
}
