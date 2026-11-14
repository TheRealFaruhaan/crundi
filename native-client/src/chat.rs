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
    /// Messages handed over while Claude works (2.9): the server keeps them
    /// until the CLI reads them (its pendingInjections), oldest first.
    pub slots: Vec<Slot>,
}

/// A message handed over to a busy chat (2.9, the web's handedOver).
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Slot {
    pub text: String,
    /// The server's id for it, once known.
    pub uuid: String,
    /// sent (not yet confirmed) | queued (can still be taken back) | started (being read)
    pub state: String,
}

/// What the handed-over drawer says, from its slots (the first that applies).
#[derive(Debug, PartialEq)]
pub enum Handed { Recallable, Started, Waiting }

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
        self.slots = session.get("pendingInjections").and_then(|p| p.as_array()).map(|p| p.iter().map(|x| Slot {
            text: s(x, "text"), uuid: s(x, "uuid"), state: { let st = s(x, "state"); if st.is_empty() { "sent".into() } else { st } },
        }).collect()).unwrap_or_default();
    }

    /// A message handed over from here: its slot, until the server says.
    pub fn add_slot(&mut self, text: &str) {
        self.slots.push(Slot { text: text.into(), uuid: String::new(), state: "sent".into() });
    }

    /// The send's answer for a slot: injected (the oldest slot with that text
    /// and no id takes the id), or not (the server was idle: it's in the log,
    /// no slot).
    pub fn sent(&mut self, text: &str, injected: bool, uuid: &str) {
        let i = self.slots.iter().position(|x| x.text == text && x.uuid.is_empty());
        match (i, injected) {
            (Some(i), false) => { self.slots.remove(i); }
            (Some(i), true) => { if !uuid.is_empty() { self.slots[i].uuid = uuid.into(); } }
            _ => {}
        }
    }

    /// The send was refused: its slot goes (the text is given back).
    pub fn drop_slot(&mut self, text: &str) {
        if let Some(i) = self.slots.iter().position(|x| x.text == text) { self.slots.remove(i); }
    }

    /// The slots that can still be taken back: (uuid, text), oldest first.
    pub fn recallable(&self) -> Vec<(String, String)> {
        self.slots.iter().filter(|x| !x.uuid.is_empty() && x.state == "queued").map(|x| (x.uuid.clone(), x.text.clone())).collect()
    }

    /// The handed-over drawer: what it says, and its body (every slot's text,
    /// oldest first). None: no drawer.
    pub fn handed(&self) -> Option<(Handed, String)> {
        if self.slots.is_empty() { return None; }
        let body = self.slots.iter().map(|x| x.text.as_str()).collect::<Vec<_>>().join("\n");
        let kind = if !self.recallable().is_empty() { Handed::Recallable } else if self.slots.iter().any(|x| x.state == "started") { Handed::Started } else { Handed::Waiting };
        Some((kind, body))
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
            // The handed-over messages' life (2.9): the CLI's word on one, by
            // its id or else the oldest with that text and no id.
            "queued-state" => {
                let (uuid, st, text) = (s(event, "uuid"), s(event, "state"), s(event, "text"));
                let i = self.slots.iter().position(|x| !uuid.is_empty() && x.uuid == uuid)
                    .or_else(|| self.slots.iter().position(|x| x.uuid.is_empty() && x.text == text));
                let Some(i) = i else { return false };
                if self.slots[i].uuid.is_empty() { self.slots[i].uuid = uuid; }
                match st.as_str() {
                    "queued" | "started" => self.slots[i].state = st,
                    "cancelled" | "discarded" | "refused" => { self.slots.remove(i); }
                    _ => return false,
                }
                true
            }
            // Read by the CLI: the slot with that text, else the oldest.
            "injected" => {
                if self.slots.is_empty() { return false; }
                let text = s(event, "text");
                let i = self.slots.iter().position(|x| x.text == text).unwrap_or(0);
                self.slots.remove(i);
                true
            }
            // Gone with the session: beyond recall.
            "exit" => { self.slots.clear(); true }
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

/// The box after a take-back (2.9): the cancelled messages (oldest first),
/// then the held text, joined with newlines; then the box's own text, if
/// any (trimmed), after a newline.
pub fn taken_back(cancelled: &[String], held: &str, box_text: &str) -> String {
    let mut parts: Vec<&str> = cancelled.iter().map(String::as_str).collect();
    if !held.is_empty() { parts.push(held); }
    let mut out = parts.join("\n");
    let own = box_text.trim();
    if !own.is_empty() { if !out.is_empty() { out.push('\n'); } out.push_str(own); }
    out
}

/// A refused send's text, given back in front of what the box holds.
pub fn given_back(text: &str, box_text: &str) -> String {
    if box_text.is_empty() { text.to_string() } else { format!("{text}\n{box_text}") }
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

    #[test]
    fn handed_over_messages() {
        let mut c = Conversation::default();
        c.load(&json!({ "id": "c", "messages": [], "pendingInjections": [{ "text": "from before", "uuid": "u0", "state": "queued" }] }));
        assert_eq!(c.handed(), Some((Handed::Recallable, "from before".into())));
        c.add_slot("one");
        c.sent("one", true, "u1");
        assert_eq!(c.slots[1].uuid, "u1");
        // Not injected: the server was idle, no slot.
        c.add_slot("two");
        c.sent("two", false, "u2");
        assert_eq!(c.slots.len(), 2);
        // queued-state: by id; started is too late; cancelled goes.
        assert!(c.apply(&json!({ "type": "queued-state", "uuid": "u1", "state": "queued", "text": "one" })));
        assert_eq!(c.recallable(), vec![("u0".into(), "from before".into()), ("u1".into(), "one".into())]);
        assert!(c.apply(&json!({ "type": "queued-state", "uuid": "u0", "state": "started", "text": "from before" })));
        assert!(c.apply(&json!({ "type": "queued-state", "uuid": "u1", "state": "cancelled", "text": "one" })));
        assert_eq!(c.handed(), Some((Handed::Started, "from before".into())));
        // A slot with no id adopts the id of the event with its text.
        c.add_slot("three");
        assert!(c.apply(&json!({ "type": "queued-state", "uuid": "u3", "state": "queued", "text": "three" })));
        assert_eq!(c.slots[1].uuid, "u3");
        // Read: by text, else the oldest.
        assert!(c.apply(&json!({ "type": "injected", "text": "three" })));
        assert_eq!(c.slots.len(), 1);
        assert!(c.apply(&json!({ "type": "injected", "text": "something else", "assumed": true })));
        assert_eq!(c.handed(), None);
        // Waiting: sent, not yet confirmed.
        c.add_slot("four");
        assert_eq!(c.handed(), Some((Handed::Waiting, "four".into())));
        assert!(c.apply(&json!({ "type": "exit", "code": 0 })));
        assert!(c.slots.is_empty());
    }

    #[test]
    fn take_back_and_give_back() {
        assert_eq!(taken_back(&["h".into()], "k", ""), "h\nk");
        assert_eq!(taken_back(&[], "x", "y "), "x\ny");
        assert_eq!(taken_back(&["a".into(), "b".into()], "", ""), "a\nb");
        assert_eq!(taken_back(&[], "", "own"), "own");
        assert_eq!(given_back("sent", "typed"), "sent\ntyped");
        assert_eq!(given_back("sent", ""), "sent");
    }
}
