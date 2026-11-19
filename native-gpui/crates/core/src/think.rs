//! Thinking blocks (2.15, specs/thinking.md): when a block is open, as the
//! web's thinkUser / thinkAuto / thinkingChanged (claude-chat.js). The
//! reader's click wins over every rule; a block that starts streaming opens
//! and folds every other block the rules had opened; a finished one folds
//! 5 s later; one first seen not streaming is folded. Pure: the app keeps
//! one per chat and runs the 5 s timer.

use std::collections::HashMap;

/// How long a finished block stays open.
pub const LINGER_MS: u64 = 5000;

#[derive(Default, Debug, Clone, PartialEq)]
pub struct Think {
    /// The reader's choice, by entry id.
    user: HashMap<String, bool>,
    /// The rules' choice, by entry id.
    auto: HashMap<String, bool>,
}

impl Think {
    /// A block started streaming: open, and the others the rules opened fold.
    pub fn started(&mut self, id: &str) {
        for v in self.auto.values_mut() { *v = false; }
        self.auto.insert(id.to_string(), true);
    }

    /// It finished: the app folds it after LINGER_MS (unless chosen).
    pub fn linger_over(&mut self, id: &str) {
        self.auto.insert(id.to_string(), false);
    }

    /// A head clicked: the opposite of what it shows, and that sticks.
    pub fn toggle(&mut self, id: &str) {
        let now = self.open(id);
        self.user.insert(id.to_string(), !now);
    }

    pub fn open(&self, id: &str) -> bool {
        self.user.get(id).or_else(|| self.auto.get(id)).copied().unwrap_or(false)
    }
}

/// "Thought", or "Thought for ~640 tokens" / "~1.2k tokens".
pub fn thought(tokens: u64) -> String {
    if tokens == 0 { return "Thought".into(); }
    let n = if tokens < 1000 { tokens.to_string() } else { format!("{:.1}k", tokens as f64 / 1000.0) };
    format!("Thought for ~{n} tokens")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wording() {
        assert_eq!(thought(0), "Thought");
        assert_eq!(thought(640), "Thought for ~640 tokens");
        assert_eq!(thought(999), "Thought for ~999 tokens");
        assert_eq!(thought(1000), "Thought for ~1.0k tokens");
        assert_eq!(thought(1234), "Thought for ~1.2k tokens");
    }

    #[test]
    fn rules() {
        let mut t = Think::default();
        // First seen not streaming: folded.
        assert!(!t.open("h"));
        t.started("a");
        assert!(t.open("a"));
        // A newer block folds the older at once, even one still streaming.
        t.started("b");
        assert!(!t.open("a") && t.open("b"));
        // Finished: folded after the linger.
        t.linger_over("b");
        assert!(!t.open("b"));
        // The reader's choice wins over every rule.
        t.toggle("b");
        assert!(t.open("b"));
        t.started("c");
        assert!(t.open("b") && t.open("c"));
        t.toggle("c");
        t.linger_over("c");
        assert!(!t.open("c"));
        t.started("d");
        assert!(!t.open("c"));
    }
}
