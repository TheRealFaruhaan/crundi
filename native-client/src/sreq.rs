//! Secret requests (2.12b, specs/secret-requests.md): Claude's `secret_get`
//! and `secret_run` waiting on the owner. The server's list as plain rows,
//! which card holds the field, the local checks before Approve, what is
//! sent, and what a refusal says.

use serde_json::{json, Value};

/// One waiting request. Never a value: the server sends none.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Req {
    pub id: String,
    pub name: String,
    pub project: String,
    pub reason: String,
    /// "run" (a command with the secret bound to `env`) or "get".
    pub run: bool,
    pub command: String,
    pub env: String,
    /// "pin" (6 digits, old secrets) or "passphrase".
    pub lock: String,
    /// The chat that asked ("" when none did).
    pub session: String,
}

fn st(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

/// The server's `requests` (GET /api/secrets, or the `secret-requests`
/// event), in its order (oldest first).
pub fn parse(v: &Value) -> Vec<Req> {
    v.as_array().map(|a| a.iter().filter(|r| !st(r, "id").is_empty()).map(|r| Req {
        id: st(r, "id"),
        name: st(r, "secretName"),
        project: st(r, "project"),
        reason: st(r, "reason"),
        run: st(r, "kind") == "run",
        command: st(r, "command"),
        env: st(r, "envName"),
        lock: if st(r, "lock") == "pin" { "pin".into() } else { "passphrase".into() },
        session: st(r, "sessionId"),
    }).collect()).unwrap_or_default()
}

/// Which card holds the field: the one that did while it is still there,
/// else the first (None: no requests). The bool says it was kept.
pub fn armed(list: &[Req], current: Option<&str>) -> Option<(String, bool)> {
    if let Some(c) = current { if list.iter().any(|r| r.id == c) { return Some((c.to_string(), true)); } }
    list.first().map(|r| (r.id.clone(), false))
}

/// The check before anything is sent: an empty passphrase, a PIN that is
/// not exactly six digits.
pub fn check(lock: &str, text: &str) -> Result<(), &'static str> {
    if lock == "pin" {
        let t = text.trim();
        if t.len() == 6 && t.chars().all(|c| c.is_ascii_digit()) { Ok(()) } else { Err("Enter a 6-digit PIN") }
    } else if text.is_empty() { Err("Enter the passphrase") } else { Ok(()) }
}

/// "Allow for this chat" is offered on a run a chat asked for, when the
/// server can pre-authorise and the chat is not a collaborator's.
pub fn can_allow(r: &Req, cap: bool, collaborator: bool) -> bool {
    cap && r.run && !r.session.is_empty() && !collaborator
}

/// The durations of the allow row: its key and what it reads.
pub const DURATIONS: [(&str, &str); 4] = [("15m", "15 minutes"), ("1h", "1 hour"), ("8h", "8 hours"), ("chat", "Until this chat ends")];

pub fn duration_label(key: &str) -> &'static str {
    DURATIONS.iter().find(|d| d.0 == key).map(|d| d.1).unwrap_or("1 hour")
}

/// What Approve sends.
pub fn approve_body(req: &Req, pin: &str, allow: Option<&str>) -> Value {
    let pin = if req.lock == "pin" { pin.trim() } else { pin };
    let mut b = json!({ "action": "approve", "reqId": req.id, "pin": pin });
    if let Some(d) = allow { b["allowForChat"] = json!({ "duration": d }); }
    b
}

/// The server's answer to Approve: Ok(the toast) or Err(the error line).
pub fn answer(r: Result<(u16, Value), String>, allow_sent: bool) -> Result<&'static str, String> {
    match r {
        Err(e) => Err(e),
        Ok((_, v)) if v.get("ok").and_then(|o| o.as_bool()) == Some(true) => {
            let pre = v.get("preauthorised").and_then(|p| p.as_bool()).unwrap_or(false);
            Ok(if allow_sent && pre { "Approved, and allowed for this chat" } else { "Approved" })
        }
        Ok((_, v)) => Err(v.get("error").and_then(|e| e.as_str()).filter(|e| !e.is_empty()).unwrap_or("Wrong PIN").to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> Value {
        json!([
            { "id": "a", "secretId": "s1", "secretName": "GITHUB_TOKEN", "project": "demo", "reason": "why", "kind": "run", "command": "echo hi", "envName": "SECRET", "lock": "passphrase", "sessionId": "c1" },
            { "id": "b", "secretName": "DB", "kind": "get", "lock": "pin" },
            { "secretName": "no id" }
        ])
    }

    #[test]
    fn parses_the_list() {
        let l = parse(&list());
        assert_eq!(l.len(), 2);
        assert_eq!(l[0], Req { id: "a".into(), name: "GITHUB_TOKEN".into(), project: "demo".into(), reason: "why".into(), run: true, command: "echo hi".into(), env: "SECRET".into(), lock: "passphrase".into(), session: "c1".into() });
        assert!(!l[1].run);
        assert_eq!(l[1].lock, "pin");
        assert_eq!(l[1].session, "");
        assert!(parse(&Value::Null).is_empty());
    }

    #[test]
    fn arms_the_kept_or_the_first() {
        let l = parse(&list());
        assert_eq!(armed(&l, Some("b")), Some(("b".into(), true)));
        assert_eq!(armed(&l, Some("gone")), Some(("a".into(), false)));
        assert_eq!(armed(&l, None), Some(("a".into(), false)));
        assert_eq!(armed(&[], Some("a")), None);
    }

    #[test]
    fn checks_before_sending() {
        assert_eq!(check("passphrase", ""), Err("Enter the passphrase"));
        assert_eq!(check("passphrase", " "), Ok(()));
        assert_eq!(check("pin", "12345"), Err("Enter a 6-digit PIN"));
        assert_eq!(check("pin", "12345a"), Err("Enter a 6-digit PIN"));
        assert_eq!(check("pin", " 123456 "), Ok(()));
    }

    #[test]
    fn allow_only_for_a_chats_run() {
        let l = parse(&list());
        assert!(can_allow(&l[0], true, false));
        assert!(!can_allow(&l[0], false, false));
        assert!(!can_allow(&l[0], true, true));
        assert!(!can_allow(&l[1], true, false));
        let mut r = l[0].clone();
        r.session.clear();
        assert!(!can_allow(&r, true, false));
    }

    #[test]
    fn the_body_and_the_answer() {
        let l = parse(&list());
        assert_eq!(approve_body(&l[0], " pass ", None), json!({ "action": "approve", "reqId": "a", "pin": " pass " }));
        assert_eq!(approve_body(&l[1], " 123456", Some("8h")), json!({ "action": "approve", "reqId": "b", "pin": "123456", "allowForChat": { "duration": "8h" } }));
        assert_eq!(answer(Ok((200, json!({ "ok": true }))), false), Ok("Approved"));
        assert_eq!(answer(Ok((200, json!({ "ok": true, "preauthorised": true }))), true), Ok("Approved, and allowed for this chat"));
        assert_eq!(answer(Ok((200, json!({ "ok": true, "preauthorised": false }))), true), Ok("Approved"));
        assert_eq!(answer(Ok((200, json!({ "ok": false, "error": "Incorrect passphrase" }))), false), Err("Incorrect passphrase".into()));
        assert_eq!(answer(Ok((200, json!({ "ok": false }))), false), Err("Wrong PIN".into()));
        assert_eq!(answer(Err("Network error".into()), false), Err("Network error".into()));
        assert_eq!(duration_label("chat"), "Until this chat ends");
        assert_eq!(duration_label("?"), "1 hour");
    }
}
