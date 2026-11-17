//! The slash menu (2.11, specs/slash-menu.md): when it opens, what it lists,
//! what a choice puts in the box. Pure; the app (src/app.rs) shows it.

use serde_json::Value;

/// At most this many commands are listed (as the web).
pub const MAX: usize = 40;

const BTW_DESC: &str = "Ask on the side: answered from the conversation, not added to it";

/// The typed part when the whole message is one `/word` (letters, digits,
/// `_`, `:`, `-`; no space, no newline); None otherwise.
pub fn query(text: &str) -> Option<&str> {
    let q = text.strip_prefix('/')?;
    q.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '-')).then_some(q)
}

/// The commands whose name starts with the query (case-insensitive), in the
/// server's order, Crundi's /btw last when the CLI has none; at most MAX.
/// Each is (name, description); a names-only command has no description.
pub fn filter(cmds: &[Value], q: &str) -> Vec<(String, String)> {
    let mut all: Vec<(String, String)> = cmds.iter().filter_map(|c| match c {
        Value::String(n) => Some((n.clone(), String::new())),
        Value::Object(o) => Some((o.get("name")?.as_str()?.to_string(), o.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string())),
        _ => None,
    }).collect();
    if !all.iter().any(|(n, _)| n == "btw") { all.push(("btw".into(), BTW_DESC.into())); }
    let q = q.to_lowercase();
    all.into_iter().filter(|(n, _)| n.to_lowercase().starts_with(&q)).take(MAX).collect()
}

/// What a choice puts in the box.
pub fn chosen(name: &str) -> String {
    format!("/{name} ")
}

/// ↑ ↓: one step, stopping at the ends.
pub fn moved(sel: usize, by: i32, len: usize) -> usize {
    if len == 0 { return 0; }
    (sel as i64 + by as i64).clamp(0, len as i64 - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_menu() {
        assert_eq!(query("/"), Some(""));
        assert_eq!(query("/Eng:t-x_1"), Some("Eng:t-x_1"));
        assert_eq!(query("/eng t"), None);
        assert_eq!(query("/eng\n"), None);
        assert_eq!(query("hi /x"), None);
        let cmds = vec![json!("clear"), json!({ "name": "code-review", "description": "Review" }), json!({ "name": "Compact" }), json!("bugs")];
        assert_eq!(filter(&cmds, "c"), vec![("clear".into(), "".into()), ("code-review".into(), "Review".into()), ("Compact".into(), "".into())]);
        assert_eq!(filter(&cmds, "b"), vec![("bugs".into(), "".into()), ("btw".into(), BTW_DESC.into())]);
        assert!(filter(&cmds, "zz").is_empty());
        let with_btw = vec![json!("btw"), json!("b2")];
        assert_eq!(filter(&with_btw, "b").len(), 2);
        let many: Vec<Value> = (0..60).map(|i| json!(format!("a{i}"))).collect();
        assert_eq!(filter(&many, "a").len(), MAX);
        assert_eq!(chosen("clear"), "/clear ");
        assert_eq!((moved(0, -1, 3), moved(2, 1, 3), moved(1, 1, 3)), (0, 2, 2));
    }
}
