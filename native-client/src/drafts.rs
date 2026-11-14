//! Drafts (2.8, specs/attachments-drafts.md): what each chat's message box
//! keeps when it isn't sent, as the web's (claude-chat.js writeDraft /
//! readDraft / saveDraft): the chat's own draft and a copy per project, so a
//! draft outlives its chat (a closed chat, a server restart). The project's
//! copy belongs to one open chat at a time (a claim, in memory only).
//! Pure: the app keeps one per server and writes it to the saved settings
//! (src/store.rs Saved.drafts).

use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// 30 days, in ms: older drafts are dropped.
pub const TTL_MS: u64 = 30 * 24 * 3600 * 1000;

#[derive(Default, Debug, Clone)]
pub struct Drafts {
    /// "chat:<id>" and "project:<alias>" → (text, when written, ms).
    entries: HashMap<String, (String, Option<u64>)>,
    /// Project alias → the chat that owns its copy.
    claims: HashMap<String, String>,
}

impl Drafts {
    /// From the saved settings: {"chat:<id>": {v, t}, …}; a bare string is
    /// read as a draft with no time (never pruned), as the web reads one.
    pub fn from_saved(v: &Value) -> Drafts {
        let mut d = Drafts::default();
        if let Some(o) = v.as_object() {
            for (k, e) in o {
                let (text, t) = match e {
                    Value::String(s) => (s.clone(), None),
                    o => (o.get("v").and_then(|x| x.as_str()).unwrap_or("").to_string(), o.get("t").and_then(|x| x.as_u64())),
                };
                if !text.is_empty() { d.entries.insert(k.clone(), (text, t)); }
            }
        }
        d
    }

    pub fn to_saved(&self) -> Value {
        let mut o = Map::new();
        for (k, (v, t)) in &self.entries {
            o.insert(k.clone(), match t { Some(t) => json!({ "v": v, "t": t }), None => json!({ "v": v }) });
        }
        Value::Object(o)
    }

    /// A chat's own draft.
    pub fn own(&self, chat: &str) -> Option<&str> {
        self.entries.get(&format!("chat:{chat}")).map(|e| e.0.as_str())
    }

    pub fn project(&self, alias: &str) -> Option<&str> {
        self.entries.get(&format!("project:{alias}")).map(|e| e.0.as_str())
    }

    #[cfg(test)]
    pub fn owner(&self, alias: &str) -> Option<&str> {
        self.claims.get(alias).map(String::as_str)
    }

    fn put(&mut self, key: String, text: &str, now: u64) {
        if text.is_empty() { self.entries.remove(&key); } else { self.entries.insert(key, (text.to_string(), Some(now))); }
    }

    /// The box changed (typing, a path put in, a send, a refused send): its
    /// text is the chat's draft; the project's copy follows unless another
    /// chat owns it. Empty removes.
    pub fn save(&mut self, chat: &str, alias: &str, text: &str, now: u64) {
        self.put(format!("chat:{chat}"), text, now);
        if alias.is_empty() { return; }
        match self.claims.get(alias).map(String::as_str) {
            Some(owner) if owner != chat => {}
            owned => {
                if !text.is_empty() {
                    self.put(format!("project:{alias}"), text, now);
                    self.claims.insert(alias.to_string(), chat.to_string());
                } else if owned == Some(chat) {
                    self.entries.remove(&format!("project:{alias}"));
                    self.claims.remove(alias);
                }
            }
        }
    }

    /// A chat's composer is built: its own draft, else the project's copy if
    /// no open chat owns it. Found, this chat claims the copy and it is saved
    /// again (under this chat, with a new time).
    pub fn restore(&mut self, chat: &str, alias: &str, now: u64) -> Option<String> {
        self.prune(now);
        let text = self.own(chat).map(str::to_string).or_else(|| {
            if alias.is_empty() || self.claims.get(alias).is_some_and(|o| o != chat) { return None; }
            self.project(alias).map(str::to_string)
        })?;
        if !alias.is_empty() { self.claims.insert(alias.to_string(), chat.to_string()); }
        self.save(chat, alias, &text, now);
        Some(text)
    }

    /// A chat's held lines (2.9), kept with the drafts so a quit (or a kill)
    /// doesn't lose them; empty removes.
    pub fn set_held(&mut self, chat: &str, text: &str, now: u64) {
        self.put(format!("held:{chat}"), text, now);
    }

    /// Held lines saved by an earlier run, taken (once).
    pub fn take_held(&mut self, chat: &str) -> Option<String> {
        self.entries.remove(&format!("held:{chat}")).map(|e| e.0)
    }

    /// A chat's composer went (closed, another project): its claim is let go;
    /// nothing is removed.
    pub fn release(&mut self, chat: &str, alias: &str) {
        if self.claims.get(alias).is_some_and(|o| o == chat) { self.claims.remove(alias); }
    }

    /// Drop drafts older than 30 days (those with no time stay).
    pub fn prune(&mut self, now: u64) {
        self.entries.retain(|_, (_, t)| t.is_none_or(|t| now.saturating_sub(t) <= TTL_MS));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DAY: u64 = 24 * 3600 * 1000;

    #[test]
    fn drafts_as_the_web() {
        let mut d = Drafts::default();
        // Typing: own and project copy, claimed.
        d.save("a", "demo", "hello", 1);
        assert_eq!((d.own("a"), d.project("demo"), d.owner("demo")), (Some("hello"), Some("hello"), Some("a")));
        // Another chat types: its own only (a owns the copy).
        d.save("b", "demo", "other", 2);
        assert_eq!((d.own("b"), d.project("demo")), (Some("other"), Some("hello")));
        // Sent: own and the owned copy removed, the claim let go.
        d.save("a", "demo", "", 3);
        assert_eq!((d.own("a"), d.project("demo"), d.owner("demo")), (None, None, None));
        // Closing keeps everything but the claim.
        d.save("c", "demo", "keep me", 4);
        d.release("c", "demo");
        assert_eq!((d.own("c"), d.project("demo"), d.owner("demo")), (Some("keep me"), Some("keep me"), None));
        // A new chat adopts the unclaimed copy and claims it.
        assert_eq!(d.restore("n", "demo", 5).as_deref(), Some("keep me"));
        assert_eq!((d.own("n"), d.owner("demo")), (Some("keep me"), Some("n")));
        // Its own wins over the copy; a claimed copy isn't taken by another.
        assert_eq!(d.restore("b", "demo", 6).as_deref(), Some("other"));
        assert_eq!(d.owner("demo"), Some("b"));
        assert_eq!(d.restore("z", "demo", 7), None);
    }

    #[test]
    fn saved_and_pruned() {
        let now = 100 * DAY;
        let d0 = Drafts::from_saved(&json!({ "chat:old": { "v": "x", "t": now - 31 * DAY }, "chat:new": { "v": "y", "t": now - 29 * DAY }, "chat:bare": "z", "chat:empty": { "v": "" } }));
        let mut d = d0.clone();
        d.prune(now);
        assert_eq!((d.own("old"), d.own("new"), d.own("bare"), d.own("empty")), (None, Some("y"), Some("z"), None));
        let back = Drafts::from_saved(&d.to_saved());
        assert_eq!((back.own("new"), back.own("bare")), (Some("y"), Some("z")));
    }

    #[test]
    fn held_lines_kept() {
        let mut d = Drafts::default();
        d.set_held("a", "x\ny", 1);
        let back = Drafts::from_saved(&d.to_saved());
        let mut back = back;
        assert_eq!(back.take_held("a").as_deref(), Some("x\ny"));
        assert_eq!(back.take_held("a"), None);
        d.set_held("a", "", 2);
        assert_eq!(d.take_held("a"), None);
    }
}
