//! One chat's conversation: the server's entries (ui-history, then ui-event
//! entry / patch / delta / state), kept in order, and turned into what the
//! chat view shows. The shapes are the server's (src/claude-ui.js).

use crate::tools;
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
    /// The CLI's commands for the slash menu (2.11): names, or {name, description}.
    pub slash: Vec<Value>,
    /// The chat's agent records (2.14: a call that started a sub-agent is an
    /// agent row), by their toolUseId.
    pub agents: Vec<Value>,
    /// The suggestion Claude Code offers after a turn (2.17), shown while
    /// idle; and the recap (its text, why: idle | resumed).
    pub suggestion: String,
    pub recap: Option<(String, String)>,
    /// Each agent's own transcript (2.18), by its toolUseId.
    pub agent_msgs: std::collections::HashMap<String, Vec<Value>>,
    /// The goal (2.19) {condition, verdicts, looping}, and the plan {text, status}.
    pub goal: Option<Value>,
    pub plan: Option<Value>,
    /// What changed since the last [Conversation::take_changes] (v2: the
    /// store re-derives only these rows): entry ids, and whether the whole
    /// conversation was replaced (a history).
    pub changed: Vec<String>,
    pub reloaded: bool,
    /// Each entry's place in `entries`, by its id (finding one is O(1)).
    index: std::collections::HashMap<String, usize>,
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
        self.slash = session.get("slashCommands").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        self.cost = session.get("totalCostUsd").and_then(|c| c.as_f64()).unwrap_or(0.0);
        self.entries = session.get("messages").and_then(|m| m.as_array()).cloned().unwrap_or_default();
        self.reindex();
        self.reloaded = true;
        self.changed.clear();
        self.agents = session.get("agents").and_then(|m| m.as_array()).cloned().unwrap_or_default();
        self.agent_msgs = self.agents.iter().map(|a| (s(a, "toolUseId"), a.get("messages").and_then(|m| m.as_array()).cloned().unwrap_or_default())).collect();
        self.goal = session.get("goal").filter(|g| !g.is_null()).cloned();
        self.plan = session.get("plan").filter(|g| !g.is_null()).cloned();
        self.suggestion = session.pointer("/suggestion/text").and_then(|t| t.as_str()).unwrap_or("").to_string();
        self.recap = session.get("recap").filter(|r| !r.is_null()).map(|r| (s(r, "text"), s(r, "why"))).filter(|r| !r.0.is_empty());
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

    fn reindex(&mut self) {
        self.index = self.entries.iter().enumerate().filter_map(|(i, e)| { let id = s(e, "id"); (!id.is_empty()).then_some((id, i)) }).collect();
    }

    fn find_mut(&mut self, id: &str) -> Option<&mut Value> {
        let i = *self.index.get(id)?;
        self.entries.get_mut(i)
    }

    fn touched(&mut self, id: &str) {
        if !self.reloaded && !self.changed.iter().any(|c| c == id) { self.changed.push(id.to_string()); }
    }

    /// What changed since the last call: (replaced whole, the entries changed).
    pub fn take_changes(&mut self) -> (bool, Vec<String>) {
        (std::mem::take(&mut self.reloaded), std::mem::take(&mut self.changed))
    }

    /// The place of an entry in `entries`.
    pub fn position(&self, id: &str) -> Option<usize> { self.index.get(id).copied() }

    /// One ui-event. Returns true when what is shown changed.
    pub fn apply(&mut self, event: &Value) -> bool {
        match event.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "entry" => {
                let Some(e) = event.get("entry") else { return false };
                let id = s(e, "id");
                match self.find_mut(&id) {
                    Some(old) => *old = e.clone(),
                    // (The cost is the snapshot's, as the web's: it refreshes
                    // with the next snapshot, not after each turn.)
                    None => {
                        if !id.is_empty() { self.index.insert(id.clone(), self.entries.len()); }
                        self.entries.push(e.clone());
                    }
                }
                self.touched(&id);
                true
            }
            // 2.19: the goal (null: it goes); a meta carrying a plan (null: it goes).
            "goal" => { self.goal = event.get("goal").filter(|g| !g.is_null()).cloned(); true }
            "meta" if event.get("plan").is_some() => { self.plan = event.get("plan").filter(|g| !g.is_null()).cloned(); true }
            // 2.17: a suggestion (null: it goes), a recap (null: it goes).
            "suggestion" => {
                self.suggestion = event.pointer("/suggestion/text").and_then(|t| t.as_str()).unwrap_or("").to_string();
                true
            }
            "recap" => {
                self.recap = event.get("recap").filter(|r| !r.is_null()).map(|r| (s(r, "text"), s(r, "why"))).filter(|r| !r.0.is_empty());
                true
            }
            // An agent's own entry, or a patch to one (2.18).
            "agent-entry" => {
                let (Some(e), tid) = (event.get("entry"), s(event, "toolUseId")) else { return false };
                let list = self.agent_msgs.entry(tid).or_default();
                let id = s(e, "id");
                match list.iter_mut().find(|x| s(x, "id") == id && !id.is_empty()) { Some(old) => *old = e.clone(), None => list.push(e.clone()) }
                true
            }
            "agent-patch" => {
                let (tid, id) = (s(event, "toolUseId"), s(event, "id"));
                let Some(list) = self.agent_msgs.get_mut(&tid) else { return false };
                let (Some(entry), Some(patch)) = (list.iter_mut().find(|x| s(x, "id") == id), event.get("patch").and_then(|p| p.as_object())) else { return false };
                if let Some(o) = entry.as_object_mut() { for (k, v) in patch { o.insert(k.clone(), v.clone()); } }
                true
            }
            "agent" => {
                let Some(ag) = event.get("agent") else { return false };
                let tid = s(ag, "toolUseId");
                if tid.is_empty() { return false; }
                match self.agents.iter_mut().find(|x| s(x, "toolUseId") == tid) {
                    Some(old) => {
                        // A record may come as a partial update: merged.
                        if let (Some(o), Some(n)) = (old.as_object_mut(), ag.as_object()) { for (k, v) in n { o.insert(k.clone(), v.clone()); } }
                    }
                    None => self.agents.push(ag.clone()),
                }
                true
            }
            "patch" => {
                let id = s(event, "id");
                let Some(patch) = event.get("patch").and_then(|p| p.as_object()) else { return false };
                let Some(entry) = self.find_mut(&id) else { return false };
                if let Some(o) = entry.as_object_mut() {
                    for (k, v) in patch { o.insert(k.clone(), v.clone()); }
                }
                self.touched(&id);
                true
            }
            "delta" => {
                let id = s(event, "id");
                let text = s(event, "text");
                let Some(entry) = self.find_mut(&id) else { return false };
                // Appended in place (no copy of the whole text per delta).
                match entry.get_mut("text") {
                    Some(Value::String(t)) => t.push_str(&text),
                    _ => if let Some(o) = entry.as_object_mut() { o.insert("text".into(), Value::String(text)); },
                }
                self.touched(&id);
                true
            }
            "state" => {
                self.state = s(event, "state");
                // A suggestion is for an idle chat: it goes for good the
                // moment the chat is anything else (2.17).
                if self.state != "idle" { self.suggestion.clear(); }
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
            "exit" => { self.slots.clear(); self.suggestion.clear(); self.recap = None; true }
            "meta" | "init" => {
                // init replaces the slash commands (names only after a first turn).
                if let Some(c) = event.get("slashCommands").and_then(|v| v.as_array()) { self.slash = c.clone(); }
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
    /// A widget_render call (2.20): the widget's id (its screenshots row).
    pub widget: String,
    /// question: the whole entry (its questions, status, answers: 2.13).
    pub raw: Value,
    /// tool (2.14): the call's id (an agent record's key), its body's
    /// arguments (src/tools.rs), and the JSON of its input (the body of a
    /// tool with no look of its own).
    pub tool_use_id: String,
    pub body: tools::Body,
    pub args: String,
}

fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

impl Shown {
    /// One entry as the view draws it (None: not drawn).
    pub fn of(e: &Value) -> Option<Shown> { shown(e) }
}

fn shown(e: &Value) -> Option<Shown> {
    let id = s(e, "id");
    let base = Shown { id, ..Default::default() };
    Some(match s(e, "kind").as_str() {
        "user" => Shown { kind: "user", text: s(e, "text"), title: s(e, "by"), ..base },
        "assistant-text" => Shown { kind: "assistant", text: s(e, "text"), status: if e.get("streaming").and_then(|v| v.as_bool()).unwrap_or(false) { "streaming".into() } else { String::new() }, ..base },
        // 2.15: streaming while it streams; the "Thought" line's wording.
        "thinking" => Shown {
            kind: "thinking", text: s(e, "text"),
            status: if e.get("streaming").and_then(|v| v.as_bool()).unwrap_or(false) { "streaming".into() } else { String::new() },
            detail: crate::think::thought(e.get("tokens").and_then(|t| t.as_f64()).map(|t| t.max(0.0) as u64).unwrap_or(0)),
            ..base
        },
        "tool" => {
            let input = e.get("input").cloned().unwrap_or(Value::Null);
            let name = s(e, "name");
            let body = tools::body(&name, &input);
            Shown {
                kind: "tool",
                // The name as sent (an MCP tool's mcp__server__tool too).
                title: name.clone(),
                detail: tools::summary(&name, &input),
                text: tools::expand_tabs(&tools::result_shown(e.get("result"))),
                args: if body == tools::Body::Json { tools::expand_tabs(&tools::json_args(&input)) } else { String::new() },
                body,
                tool_use_id: s(e, "toolUseId"),
                // A show_* call's result (its media: 2.16m), whole.
                raw: if crate::media::is_show(&name) { json_result(e) } else { Value::Null },
                status: s(e, "status"),
                error: e.get("isError").and_then(|v| v.as_bool()).unwrap_or(false),
                note: input.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                group: s(e, "group"),
                group_summary: s(e, "groupSummary"),
                widget: if name.ends_with("widget_render") { input.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string() } else { String::new() },
                ..base
            }
        }
        // The web's permissionNode: pending, its title (or "Claude wants to
        // use <name>"), description, reason and input; closed, the name and
        // a tag (never answered / denied / allowed / always allowed, by Cru),
        // and what it asked when nobody answered.
        "permission" => {
            let name = { let d = s(e, "displayName"); if d.is_empty() { s(e, "toolName") } else { d } };
            let st = s(e, "status");
            let closed = st == "unanswered" || st == "cancelled";
            let mut tag = if st == "denied" { "denied".to_string() } else if closed { "never answered".into() } else if e.get("always").and_then(|v| v.as_bool()).unwrap_or(false) { "always allowed".into() } else { "allowed".into() };
            if s(e, "answeredBy") == "cru" && !closed { tag.push_str(" by Cru"); }
            let raw_title = s(e, "title");
            // Pending: escalated to the project owner (or a factory's
            // Manager), its approval's id; "Allow always" only with the CLI's
            // suggestions and no suppressAlwaysAllow (F110).
            let escalated = e.get("escalated").and_then(|v| v.as_bool()).unwrap_or(false);
            let pending = st == "pending";
            let always_ok = !e.get("suppressAlwaysAllow").and_then(|v| v.as_bool()).unwrap_or(false) && e.get("suggestions").and_then(|v| v.as_array()).is_some_and(|a| !a.is_empty());
            Shown {
                options: if pending && always_ok { vec!["always".into()] } else { vec![] },
                kind: "permission",
                title: if raw_title.is_empty() { format!("Claude wants to use {name}") } else { raw_title.clone() },
                text: name,
                detail: s(e, "description"),
                note: tools::strip_ansi(&s(e, "decisionReason")),
                args: tools::truncate(&serde_json::to_string_pretty(e.get("input").unwrap_or(&Value::Null)).unwrap_or_default(), 2500),
                group_summary: if pending { if !escalated { String::new() } else if s(e, "routedTo") == "manager" { "manager".into() } else { "escalated".into() } } else { tag },
                group: if closed { raw_title } else if pending && escalated { s(e, "approvalId") } else { String::new() },
                error: st == "denied" || closed,
                status: st,
                request_id: s(e, "requestId"),
                ..base
            }
        }
        // A factory's result card (2.20): drawn from the whole entry.
        "factory-result" => Shown { kind: "factory-result", status: s(e, "state"), raw: e.clone(), ..base },
        "question" => {
            let q = e.pointer("/input/questions/0").cloned().unwrap_or(Value::Null);
            Shown {
                kind: "question",
                title: s(&q, "header"),
                text: s(&q, "question"),
                options: q.get("options").and_then(|o| o.as_array()).map(|a| a.iter().map(|o| s(o, "label")).collect()).unwrap_or_default(),
                status: s(e, "status"),
                request_id: s(e, "requestId"),
                raw: e.clone(),
                ..base
            }
        }
        "result" => {
            let cost = e.get("costUsd").and_then(|v| v.as_f64());
            let secs = e.get("durationMs").and_then(|v| v.as_f64()).map(|ms| ms / 1000.0);
            let mut parts = Vec::new();
            if let Some(sec) = secs { parts.push(format!("{sec:.1}s")); }
            if let Some(c) = cost { if c > 0.0 { parts.push(format!("${c:.4}")); } }
            // A turn that did not succeed (its subtype, e.g. error_max_turns):
            // the error box, with its text or else the subtype.
            let sub = s(e, "subtype");
            let error = e.get("isError").and_then(|v| v.as_bool()).unwrap_or(false) || (!sub.is_empty() && sub != "success");
            let text = if error { let t = s(e, "text"); if t.is_empty() { sub } else { t } } else { String::new() };
            Shown { kind: "result", detail: parts.join(" · "), error, text, ..base }
        }
        "error" => Shown { kind: "error", text: s(e, "text"), error: true, ..base },
        "notice" => Shown { kind: "notice", text: s(e, "text"), ..base },
        // A goal check that said "not yet" (2.19).
        "goal-verdict" => Shown { kind: "goal-verdict", text: s(e, "text"), detail: format!("Goal not yet met · check {}", e.get("n").and_then(|n| n.as_u64()).unwrap_or(0)), ..base },
        _ => return None,
    })
}

#[cfg(test)]
mod v2_tests {
    //! v2: what changed is reported, so the store re-derives only those rows.
    use super::*;
    use serde_json::json;

    #[test]
    fn changes_are_tracked_per_entry() {
        let mut c = Conversation::default();
        c.load(&json!({ "id": "c1", "messages": [ { "id": "u1", "kind": "user", "text": "hi" } ] }));
        assert_eq!(c.take_changes(), (true, vec![]));
        assert!(c.apply(&json!({ "type": "entry", "entry": { "id": "a1", "kind": "assistant-text", "text": "He", "streaming": true } })));
        assert!(c.apply(&json!({ "type": "delta", "id": "a1", "text": "llo" })));
        assert!(c.apply(&json!({ "type": "delta", "id": "a1", "text": "!" })));
        assert_eq!(c.take_changes(), (false, vec!["a1".to_string()]));
        assert_eq!(c.entries[1]["text"], "Hello!");
        assert_eq!(c.position("a1"), Some(1));
        assert!(c.apply(&json!({ "type": "patch", "id": "u1", "patch": { "text": "hey" } })));
        assert_eq!(c.take_changes(), (false, vec!["u1".to_string()]));
        // A delta for an entry nobody has: nothing changes.
        assert!(!c.apply(&json!({ "type": "delta", "id": "zz", "text": "x" })));
        assert_eq!(c.take_changes(), (false, vec![]));
    }
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
        assert_eq!((v[2].title.as_str(), v[2].detail.as_str(), v[2].text.as_str(), v[2].status.as_str()), ("Bash", "ls -la\nmore", "a b", "done"));
        assert_eq!(c.state, "working");
    }

    #[test]
    fn agents_and_tool_names() {
        let mut c = Conversation::default();
        c.load(&json!({ "messages": [
            { "id": "t", "kind": "tool", "name": "mcp__crundi__list_services", "toolUseId": "u1", "input": {}, "status": "done", "result": "ok" },
        ], "agents": [{ "toolUseId": "u1", "kind": "agent", "subagentType": "web-fetch", "status": "running" }] }));
        let v = c.view();
        assert_eq!((v[0].title.as_str(), v[0].detail.as_str(), v[0].args.as_str(), v[0].tool_use_id.as_str()), ("mcp__crundi__list_services", "", "{}", "u1"));
        assert!(c.apply(&json!({ "type": "agent", "agent": { "toolUseId": "u1", "status": "done" } })));
        assert_eq!(c.agents.len(), 1);
        assert_eq!(c.agents[0]["status"], "done");
        assert_eq!(c.agents[0]["subagentType"], "web-fetch");
        assert!(c.apply(&json!({ "type": "agent", "agent": { "toolUseId": "u2", "kind": "task" } })));
        assert_eq!(c.agents.len(), 2);
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
        assert_eq!(v[0].title, "Claude wants to use mcp__crundi__browser_open");
        assert!(v[0].args.contains("\"url\": \"https://x\""));
        assert_eq!(v[0].group_summary, "");
        assert!(v[0].options.is_empty());
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

/// A show_* call's result as JSON text (a string, or text blocks joined).
fn json_result(e: &Value) -> Value {
    match e.get("result") {
        Some(Value::String(t)) => Value::String(t.clone()),
        Some(Value::Array(b)) => Value::String(b.iter().filter_map(|x| x.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join("")),
        _ => Value::Null,
    }
}
