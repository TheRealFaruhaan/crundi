//! Tool cards (2.14, specs/tool-cards.md): what a tool call's card shows,
//! as the web's toolSummary() and toolDetailHtml() (app/vendor/claude-chat.js):
//! the head's one-line summary, the body's arguments per tool, and the
//! result as shown. Pure.

use serde_json::Value;

/// The arguments' look in the body.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Body {
    /// Bash: the command, and its description (muted) when there is one.
    Bash { command: String, description: String },
    /// Write: the new content, one added block.
    Write(String),
    /// Edit: "- old" lines then "+ new" lines (true: added).
    Diff(Vec<(String, bool)>),
    /// TodoWrite: (text, completed | in_progress | pending).
    Todos(Vec<(String, String)>),
    /// Anything else: the input as pretty JSON.
    #[default]
    Json,
}

/// The body's JSON and Write cut, and the result's.
pub const ARGS_MAX: usize = 4000;
pub const RESULT_MAX: usize = 6000;
/// A Bash command, and the key=value fallback, in the head.
pub const SUMMARY_MAX: usize = 140;

/// "…/" and the last two parts of a long path (more than three parts).
pub fn short_path(p: &str) -> String {
    let s = p.replace('\\', "/");
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() > 3 { format!("…/{}", parts[parts.len() - 2..].join("/")) } else { s }
}

/// The first n characters, and "…" when there was more.
pub fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() > n { format!("{}…", s.chars().take(n).collect::<String>()) } else { s.to_string() }
}

/// A field as the web prints it into the summary: a string as is; nothing
/// for a missing one (the web would print "undefined"; it never happens).
fn field(input: &Value, k: &str) -> String {
    match input.get(k) {
        Some(Value::String(s)) => s.clone(),
        None | Some(Value::Null) => String::new(),
        Some(v) => v.to_string(),
    }
}

/// The head's summary of a call's arguments.
pub fn summary(name: &str, input: &Value) -> String {
    let f = |k: &str| field(input, k);
    match name {
        "Read" | "Write" | "Edit" => short_path(&f("file_path")),
        "NotebookEdit" => short_path(&f("notebook_path")),
        "Bash" => truncate(&f("command"), SUMMARY_MAX),
        "Glob" => { let p = f("path"); format!("{}{}", f("pattern"), if p.is_empty() { String::new() } else { format!(" in {}", short_path(&p)) }) }
        "Grep" => { let g = f("glob"); format!("{}{}", f("pattern"), if g.is_empty() { String::new() } else { format!(" ({g})") }) }
        "WebFetch" => f("url"),
        "WebSearch" => f("query"),
        "Task" | "Agent" => { let d = f("description"); if d.is_empty() { f("subagent_type") } else { d } }
        "TodoWrite" => format!("{} items", input.get("todos").and_then(|t| t.as_array()).map(|t| t.len()).unwrap_or(0)),
        "Skill" => f("skill"),
        _ => match input.as_object() {
            Some(o) if !o.is_empty() => truncate(&o.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" "), SUMMARY_MAX),
            _ => String::new(),
        },
    }
}

/// The body's arguments.
pub fn body(name: &str, input: &Value) -> Body {
    let f = |k: &str| field(input, k);
    match name {
        "Bash" => Body::Bash { command: f("command"), description: f("description") },
        // (A <pre> drops one trailing newline: the band has no empty last line.)
        "Write" => Body::Write(truncate(drop_last_newline(&f("content")), ARGS_MAX)),
        "Edit" => {
            let mut lines: Vec<(String, bool)> = f("old_string").split('\n').map(|l| (format!("- {l}"), false)).collect();
            lines.extend(f("new_string").split('\n').map(|l| (format!("+ {l}"), true)));
            Body::Diff(lines)
        }
        "TodoWrite" => Body::Todos(input.get("todos").and_then(|t| t.as_array()).map(|ts| ts.iter().map(|t| {
            let status = field(t, "status");
            let active = field(t, "activeForm");
            let text = if status == "in_progress" && !active.is_empty() { active } else { field(t, "content") };
            let state = match status.as_str() { "completed" => "completed", "in_progress" => "in_progress", _ => "pending" };
            (text, state.to_string())
        }).collect()).unwrap_or_default()),
        _ => Body::Json,
    }
}

/// A <pre>'s text drops one trailing newline.
pub fn drop_last_newline(s: &str) -> &str {
    s.strip_suffix('\n').unwrap_or(s)
}

/// The body's JSON: the input pretty-printed (2 spaces), cut.
pub fn json_args(input: &Value) -> String {
    let v = if input.is_null() { Value::Object(Default::default()) } else { input.clone() };
    truncate(&serde_json::to_string_pretty(&v).unwrap_or_default(), ARGS_MAX)
}

/// ANSI colour and cursor escapes out (ESC [ … letter).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1b' && it.peek() == Some(&'[') {
            it.next();
            let mut ended = false;
            let mut held = String::from("\x1b[");
            while let Some(&n) = it.peek() {
                if n.is_ascii_digit() || n == ';' || n == '?' { held.push(n); it.next(); continue; }
                if n.is_ascii_alphabetic() { it.next(); ended = true; }
                break;
            }
            if !ended { out.push_str(&held); }
        } else {
            out.push(c);
        }
    }
    out
}

/// A tool result: text, or content blocks (text joined; others as [type]).
pub fn result_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(t)) => t.clone(),
        Some(Value::Array(blocks)) => blocks.iter().map(|b| match b.get("text").and_then(|t| t.as_str()) {
            Some(t) => t.to_string(),
            None => format!("[{}]", b.get("type").and_then(|t| t.as_str()).unwrap_or("")),
        }).collect::<Vec<_>>().join("\n"),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// The result as the body shows it: escapes out, cut.
pub fn result_shown(v: Option<&Value>) -> String {
    truncate(drop_last_newline(&strip_ansi(&result_text(v))), RESULT_MAX)
}

/// Tabs to the next stop of 8 (Slint's text has no tab stops).
pub fn expand_tabs(s: &str) -> String {
    if !s.contains('\t') { return s.to_string(); }
    let mut out = String::with_capacity(s.len() + 16);
    let mut col = 0usize;
    for c in s.chars() {
        match c {
            '\t' => { let n = 8 - col % 8; out.extend(std::iter::repeat(' ').take(n)); col += n; }
            '\n' => { out.push(c); col = 0; }
            _ => { out.push(c); col += 1; }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summaries_per_kind() {
        let p = "/tmp/crundi-devtest-4810/proj/app.js";
        assert_eq!(short_path(p), "…/proj/app.js");
        assert_eq!(short_path("a/b/c"), "a/b/c");
        assert_eq!(short_path("C:\\x\\y\\z\\w.txt"), "…/z/w.txt");
        assert_eq!(summary("Read", &json!({ "file_path": p })), "…/proj/app.js");
        assert_eq!(summary("Edit", &json!({ "file_path": p })), "…/proj/app.js");
        assert_eq!(summary("NotebookEdit", &json!({ "notebook_path": "/a/b/notebooks/analysis.ipynb" })), "…/notebooks/analysis.ipynb");
        assert_eq!(summary("Bash", &json!({ "command": "ls -la" })), "ls -la");
        let long = "x".repeat(150);
        assert_eq!(summary("Bash", &json!({ "command": long })), format!("{}…", "x".repeat(140)));
        assert_eq!(summary("Glob", &json!({ "pattern": "**/*.md", "path": "/tmp/x/proj/docs" })), "**/*.md in …/proj/docs");
        assert_eq!(summary("Glob", &json!({ "pattern": "*.rs" })), "*.rs");
        assert_eq!(summary("Grep", &json!({ "pattern": "port", "glob": "*.js" })), "port (*.js)");
        assert_eq!(summary("WebFetch", &json!({ "url": "https://example.com/docs" })), "https://example.com/docs");
        assert_eq!(summary("WebSearch", &json!({ "query": "slint" })), "slint");
        assert_eq!(summary("Task", &json!({ "description": "Count lines", "subagent_type": "x" })), "Count lines");
        assert_eq!(summary("Agent", &json!({ "subagent_type": "general-purpose" })), "general-purpose");
        assert_eq!(summary("TodoWrite", &json!({ "todos": [{}, {}, {}, {}] })), "4 items");
        assert_eq!(summary("Skill", &json!({ "skill": "crundi" })), "crundi");
        assert_eq!(summary("mcp__crundi__list_services", &json!({})), "");
        assert_eq!(summary("mcp__x__kanban_add_task", &json!({ "title": "Write tests", "column": "backlog" })), "title=\"Write tests\" column=\"backlog\"");
        let q = summary("AskUserQuestion", &json!({ "questions": [{ "header": "Database", "question": "Which?" }] }));
        assert!(q.starts_with("questions=[{\"header\":\"Database\""), "{q}");
    }

    #[test]
    fn bodies() {
        assert_eq!(body("Bash", &json!({ "command": "ls", "description": "List" })), Body::Bash { command: "ls".into(), description: "List".into() });
        assert_eq!(body("Write", &json!({ "content": "y".repeat(4001) })), Body::Write(format!("{}…", "y".repeat(4000))));
        assert_eq!(body("Edit", &json!({ "old_string": "a\nb", "new_string": "" })), Body::Diff(vec![("- a".into(), false), ("- b".into(), false), ("+ ".into(), true)]));
        assert_eq!(body("TodoWrite", &json!({ "todos": [
            { "content": "Done one", "status": "completed" },
            { "content": "Doing", "activeForm": "Doing it now", "status": "in_progress" },
            { "content": "Later", "status": "pending" } ] })),
            Body::Todos(vec![("Done one".into(), "completed".into()), ("Doing it now".into(), "in_progress".into()), ("Later".into(), "pending".into())]));
        assert_eq!(body("Write", &json!({ "content": "# Changelog\n\n- Port moved to 4000.\n" })), Body::Write("# Changelog\n\n- Port moved to 4000.".into()));
        assert_eq!(body("Read", &json!({})), Body::Json);
        assert_eq!(json_args(&json!({})), "{}");
        assert_eq!(json_args(&Value::Null), "{}");
        assert_eq!(json_args(&json!({ "file_path": "/a", "limit": 5 })), "{\n  \"file_path\": \"/a\",\n  \"limit\": 5\n}");
        assert!(json_args(&json!({ "x": "z".repeat(5000) })).ends_with('…'));
    }

    #[test]
    fn results() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m and \x1b[?25lplain"), "red and plain");
        assert_eq!(result_text(Some(&json!([{ "type": "text", "text": "a" }, { "type": "image" }]))), "a\n[image]");
        let r = result_shown(Some(&json!("n".repeat(6001))));
        assert_eq!(r.chars().count(), 6001);
        assert!(r.ends_with('…'));
        assert_eq!(expand_tabs("1\tx\n12345678\ty"), "1       x\n12345678        y");
    }
}
