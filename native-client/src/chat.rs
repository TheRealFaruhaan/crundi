//! One chat's conversation: the server's entries (ui-history, then ui-event
//! entry / patch / delta / state), kept in order, and turned into what the
//! chat view shows. The shapes are the server's (src/claude-ui.js).

use serde_json::Value;

#[derive(Default)]
pub struct Conversation {
    pub id: String,
    pub entries: Vec<Value>,
    /// idle | working | waiting | needs-input (the agent's state).
    pub state: String,
    pub title: String,
    pub model: String,
    /// For the composer's meta row: Claude's session id, the permission mode, the cost so far.
    pub session_id: String,
    pub mode: String,
    pub cost: f64,
    /// Launched with skip permissions: Bypass all is in its list of modes.
    pub skip: bool,
}

impl Conversation {
    /// The whole snapshot (ui-history, or a history event).
    pub fn load(&mut self, session: &Value) {
        self.id = s(session, "id");
        self.title = s(session, "title");
        self.model = s(session, "model");
        self.state = s(session, "state");
        self.session_id = s(session, "sessionId");
        self.mode = s(session, "permissionMode");
        self.skip = session.get("skipPermissions").and_then(|v| v.as_bool()).unwrap_or(false);
        self.cost = session.get("totalCostUsd").and_then(|c| c.as_f64()).unwrap_or(0.0);
        self.entries = session.get("messages").and_then(|m| m.as_array()).cloned().unwrap_or_default();
    }

    /// One ui-event. Returns true when what is shown changed.
    pub fn apply(&mut self, event: &Value) -> bool {
        match event.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "entry" => {
                let Some(e) = event.get("entry") else { return false };
                let id = s(e, "id");
                match self.entries.iter_mut().find(|x| s(x, "id") == id && !id.is_empty()) {
                    Some(old) => *old = e.clone(),
                    // (The cost is the snapshot's, as the web's: it refreshes
                    // with the next snapshot, not after each turn.)
                    None => self.entries.push(e.clone()),
                }
                true
            }
            "patch" => {
                let id = s(event, "id");
                let (Some(entry), Some(patch)) = (self.entries.iter_mut().find(|x| s(x, "id") == id), event.get("patch").and_then(|p| p.as_object())) else { return false };
                if let Some(o) = entry.as_object_mut() {
                    for (k, v) in patch { o.insert(k.clone(), v.clone()); }
                }
                true
            }
            "delta" => {
                let id = s(event, "id");
                let text = s(event, "text");
                let Some(entry) = self.entries.iter_mut().find(|x| s(x, "id") == id) else { return false };
                let joined = format!("{}{}", s(entry, "text"), text);
                if let Some(o) = entry.as_object_mut() { o.insert("text".into(), Value::String(joined)); }
                true
            }
            "state" => {
                self.state = s(event, "state");
                true
            }
            "history" => {
                if let Some(sess) = event.get("session") { self.load(sess); }
                true
            }
            "meta" | "init" => {
                if let Some(m) = event.get("model").and_then(|m| m.as_str()) { self.model = m.to_string(); }
                // A mode outside this chat's list is ignored (the web's setModeIfKnown).
                if let Some(m) = event.get("permissionMode").and_then(|m| m.as_str()) { if crate::head::modes(self.skip).contains(&m) { self.mode = m.to_string(); } }
                if let Some(m) = event.get("sessionId").and_then(|m| m.as_str()) { self.session_id = m.to_string(); }
                true
            }
            _ => false,
        }
    }

    /// The entries the view shows, in order.
    pub fn view(&self) -> Vec<Shown> {
        self.entries.iter().filter_map(shown).collect()
    }
}

/// An entry as the chat view draws it (ui/chat.slint EntryView).
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Shown {
    pub id: String,
    /// user | assistant | thinking | tool | permission | question | result | error | notice
    pub kind: &'static str,
    pub text: String,
    pub title: String,
    pub detail: String,
    /// tool: running | done; permission / question: pending | allowed | denied …
    pub status: String,
    pub error: bool,
    pub request_id: String,
    /// question: the first question's choices.
    pub options: Vec<String>,
    /// tool: Claude's own description of the call.
    pub note: String,
    /// tool: the batch Claude Code summed up in one line (its id, and the line).
    pub group: String,
    pub group_summary: String,
}

fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

const RESULT_MAX: usize = 4000;

fn clip(t: &str, max: usize) -> String {
    if t.chars().count() <= max { return t.to_string(); }
    let mut out: String = t.chars().take(max).collect();
    out.push_str("\n…");
    out
}

/// A tool result is text, or a list of content blocks.
fn result_text(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(t)) => t.clone(),
        Some(Value::Array(blocks)) => blocks.iter().filter_map(|b| b.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join("\n"),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// The one line that says what a tool call does.
pub fn tool_summary(name: &str, input: &Value) -> String {
    let pick = |keys: &[&str]| keys.iter().find_map(|k| input.get(*k).and_then(|v| v.as_str()).map(str::to_string));
    let line = match name {
        "Bash" => pick(&["command"]),
        "Read" | "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => pick(&["file_path", "notebook_path"]),
        "Grep" | "Glob" => pick(&["pattern"]),
        "WebFetch" => pick(&["url"]),
        "WebSearch" => pick(&["query"]),
        "Task" | "Agent" => pick(&["description", "prompt"]),
        "TodoWrite" => Some("Update the to-do list".into()),
        _ => None,
    };
    let line = line.or_else(|| input.as_object().and_then(|o| o.values().find_map(|v| v.as_str().map(str::to_string)))).unwrap_or_default();
    line.lines().next().unwrap_or("").chars().take(200).collect()
}

/// "mcp__crundi__browser_open" reads better as "browser_open (crundi)".
fn tool_title(e: &Value) -> String {
    let display = s(e, "displayName");
    if !display.is_empty() { return display; }
    let name = s(e, "name");
    let name = if name.is_empty() { s(e, "toolName") } else { name };
    match name.strip_prefix("mcp__").and_then(|r| r.split_once("__")) {
        Some((server, tool)) => format!("{tool} ({server})"),
        None => name,
    }
}

fn shown(e: &Value) -> Option<Shown> {
    let id = s(e, "id");
    let base = Shown { id, ..Default::default() };
    Some(match s(e, "kind").as_str() {
        "user" => Shown { kind: "user", text: s(e, "text"), title: s(e, "by"), ..base },
        "assistant-text" => Shown { kind: "assistant", text: s(e, "text"), status: if e.get("streaming").and_then(|v| v.as_bool()).unwrap_or(false) { "streaming".into() } else { String::new() }, ..base },
        "thinking" => Shown { kind: "thinking", text: s(e, "text"), ..base },
        "tool" => {
            let input = e.get("input").cloned().unwrap_or(Value::Null);
            Shown {
                kind: "tool",
                title: tool_title(e),
                detail: tool_summary(&s(e, "name"), &input),
                text: clip(&result_text(e.get("result")), RESULT_MAX),
                status: s(e, "status"),
                error: e.get("isError").and_then(|v| v.as_bool()).unwrap_or(false),
                note: input.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                group: s(e, "group"),
                group_summary: s(e, "groupSummary"),
                ..base
            }
        }
        "permission" => Shown {
            kind: "permission",
            title: { let t = s(e, "title"); if t.is_empty() { format!("Allow {}?", tool_title(e)) } else { t } },
            detail: { let d = s(e, "description"); if d.is_empty() { tool_summary(&s(e, "toolName"), e.get("input").unwrap_or(&Value::Null)) } else { d } },
            status: s(e, "status"),
            request_id: s(e, "requestId"),
            ..base
        },
        "question" => {
            let q = e.pointer("/input/questions/0").cloned().unwrap_or(Value::Null);
            Shown {
                kind: "question",
                title: s(&q, "header"),
                text: s(&q, "question"),
                options: q.get("options").and_then(|o| o.as_array()).map(|a| a.iter().map(|o| s(o, "label")).collect()).unwrap_or_default(),
                status: s(e, "status"),
                request_id: s(e, "requestId"),
                ..base
            }
        }
        "result" => {
            let cost = e.get("costUsd").and_then(|v| v.as_f64());
            let secs = e.get("durationMs").and_then(|v| v.as_f64()).map(|ms| ms / 1000.0);
            let mut parts = Vec::new();
            if let Some(sec) = secs { parts.push(format!("{sec:.1}s")); }
            if let Some(c) = cost { if c > 0.0 { parts.push(format!("${c:.4}")); } }
            let error = e.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
            Shown { kind: "result", detail: parts.join(" · "), error, text: if error { s(e, "text") } else { String::new() }, ..base }
        }
        "error" => Shown { kind: "error", text: s(e, "text"), error: true, ..base },
        "notice" => Shown { kind: "notice", text: s(e, "text"), ..base },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn history_then_events() {
        let mut c = Conversation::default();
        c.load(&json!({ "id": "c1", "title": "T", "state": "idle", "messages": [
            { "id": "1", "kind": "user", "text": "hi" },
            { "id": "2", "kind": "assistant-text", "text": "Hel", "streaming": true },
        ]}));
        assert!(c.apply(&json!({ "type": "delta", "id": "2", "text": "lo" })));
        assert!(c.apply(&json!({ "type": "patch", "id": "2", "patch": { "streaming": false } })));
        assert!(c.apply(&json!({ "type": "entry", "entry": { "id": "3", "kind": "tool", "name": "Bash", "input": { "command": "ls -la\nmore" }, "status": "running" } })));
        assert!(c.apply(&json!({ "type": "patch", "id": "3", "patch": { "status": "done", "result": [{ "type": "text", "text": "a b" }] } })));
        assert!(c.apply(&json!({ "type": "state", "state": "working" })));
        let v = c.view();
        assert_eq!(v.len(), 3);
        assert_eq!(v[1].text, "Hello");
        assert_eq!(v[1].status, "");
        assert_eq!((v[2].title.as_str(), v[2].detail.as_str(), v[2].text.as_str(), v[2].status.as_str()), ("Bash", "ls -la", "a b", "done"));
        assert_eq!(c.state, "working");
    }

    #[test]
    fn approvals_and_questions() {
        let mut c = Conversation::default();
        c.load(&json!({ "messages": [
            { "id": "p", "kind": "permission", "requestId": "r1", "toolName": "mcp__crundi__browser_open", "input": { "url": "https://x" }, "status": "pending" },
            { "id": "q", "kind": "question", "requestId": "r2", "status": "pending", "input": { "questions": [{ "header": "Pick", "question": "Which?", "options": [{ "label": "A" }, { "label": "B" }] }] } },
            { "id": "g", "kind": "gone" },
        ]}));
        let v = c.view();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].title, "Allow browser_open (crundi)?");
        assert_eq!(v[0].detail, "https://x");
        assert_eq!(v[1].options, vec!["A", "B"]);
        assert_eq!(v[1].request_id, "r2");
    }
}
