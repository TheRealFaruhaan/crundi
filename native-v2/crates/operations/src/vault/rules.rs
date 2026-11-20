//! The Secrets tab's pure rules (the Slint client's src/ops/vault.rs, the
//! web's renderSecrets and crundi-secret-lock.js): the passphrase rule and
//! its hint, the add form's and Edit's Save, the rows' words, the server's
//! answers.

use serde_json::{json, Value};

pub const RULE: &str = "At least 8 characters, and not only digits";
pub const UNSUPPORTED: &str = "This server does not support this yet. Update the server to use it.";

/// A field as text ("" for none; a number or bool as written).
pub fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(x)) => x.clone(),
        Some(Value::Null) | None => String::new(),
        Some(o) => o.to_string(),
    }
}

/// JavaScript's truthiness of a field.
pub fn truthy(v: &Value, k: &str) -> bool {
    match v.get(k) {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(x)) => !x.is_empty(),
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(_) => true,
    }
}

/// A new lock: 8 characters or more, and not only digits (the server's rule).
pub fn passphrase_ok(t: &str) -> bool {
    t.chars().count() >= 8 && t.chars().any(|c| !c.is_ascii_digit())
}

/// The hint under a new passphrase and its confirmation: (text, warns).
pub fn new_lock_hint(a: &str, b: &str) -> (String, bool) {
    let rule = format!("{RULE}.");
    if a.is_empty() { return (rule, false); }
    if !passphrase_ok(a) { return (rule, true); }
    if !b.is_empty() && a != b { return ("The two do not match.".into(), true); }
    (rule, false)
}

/// The add form's Save: a name, a value, a good passphrase typed twice.
pub fn add_ok(name: &str, value: &str, a: &str, b: &str) -> bool {
    passphrase_ok(a) && a == b && !name.trim().is_empty() && !value.is_empty()
}

/// A secret's or a request's lock kind: "passphrase", else "pin".
pub fn lock_of(v: &Value) -> &'static str {
    if s(v, "lock") == "passphrase" { "passphrase" } else { "pin" }
}

/// A request's meta line (.req-meta).
pub fn req_meta(r: &Value) -> String {
    let p = s(r, "project");
    let why = s(r, "reason");
    format!("{}{}", if p.is_empty() { String::new() } else { format!("project: {p} · ") }, if why.is_empty() { "Claude requested access".into() } else { why })
}

/// A secret's tag and its title: an old lock to update (needsUpgrade).
pub fn tag_of(v: &Value) -> (&'static str, &'static str) {
    if !truthy(v, "needsUpgrade") { return ("", ""); }
    if lock_of(v) == "passphrase" { ("Old lock", "Sealed with an older, weaker key. Edit it to seal it again.") }
    else { ("Old PIN", "Locked with an old 6-digit PIN. Edit it to set a passphrase.") }
}

/// The PIN modal's words for a lock.
pub fn ask_word(lock: &str) -> &'static str { if lock == "passphrase" { "passphrase" } else { "PIN" } }

/// Edit's Save: what changes, and whether it may be sent.
pub struct EditForm<'a> { pub name0: &'a str, pub desc0: &'a str, pub lock: &'a str, pub name: &'a str, pub desc: &'a str, pub value: &'a str, pub relock: bool, pub new: &'a str, pub new2: &'a str, pub cur: &'a str }

impl EditForm<'_> {
    pub fn revalue(&self) -> bool { !self.value.is_empty() }
    pub fn meta(&self) -> bool { self.name.trim() != self.name0 || self.desc != self.desc0 }
    pub fn ok(&self) -> bool {
        let need_lock = self.relock || self.revalue();
        (!self.relock || (passphrase_ok(self.new) && self.new == self.new2))
            && (!need_lock || crundi_core::secrets::lock_ok(self.lock, self.cur))
            && (self.relock || self.revalue() || self.meta())
            && !self.name.trim().is_empty()
    }
    /// The body Save sends: `updateMeta` when only the name or description
    /// changed (no lock needed), else `edit` with the current lock.
    pub fn body(&self, id: &str) -> Value {
        if !self.relock && !self.revalue() {
            return json!({ "action": "updateMeta", "id": id, "name": self.name.trim(), "description": self.desc });
        }
        let mut b = json!({ "action": "edit", "id": id, "pin": self.cur });
        if self.meta() { b["name"] = json!(self.name.trim()); b["description"] = json!(self.desc); }
        if self.relock { b["newPin"] = json!(self.new); }
        if self.revalue() { b["value"] = json!(self.value); }
        b
    }
}

/// An Edit refusal about the lock (the current lock field takes the focus again).
pub fn lockish(e: &str) -> bool {
    let l = e.to_lowercase();
    l.contains("incorrect") || l.contains("pin must") || l.ends_with("passphrase")
}

/// A POST /api/secrets answer as the lock module reads it: a 404 or an
/// unknown action is "not supported yet"; a failure is { ok: false, error }.
pub fn answer(r: Result<Value, String>) -> Value {
    match r {
        Err(e) if e == "Not supported by this server" => json!({ "ok": false, "error": UNSUPPORTED, "unsupported": true }),
        Err(e) => json!({ "ok": false, "error": e }),
        Ok(v) if !ok(&v) && s(&v, "error").contains("Unknown secrets action") => json!({ "ok": false, "error": UNSUPPORTED, "unsupported": true }),
        Ok(v) => v,
    }
}

pub fn ok(v: &Value) -> bool { v.get("ok").and_then(|o| o.as_bool()) == Some(true) }
pub fn err_or(v: &Value, dflt: &str) -> String { let e = s(v, "error"); if e.is_empty() { dflt.into() } else { e } }

#[cfg(test)]
mod tests {
    use super::{add_ok, answer, ask_word, lock_of, lockish, new_lock_hint, passphrase_ok, req_meta, tag_of, EditForm, RULE, UNSUPPORTED};
    use serde_json::json;

    #[::core::prelude::v1::test]
    fn the_passphrase_rule() {
        assert!(!passphrase_ok("1234567"));
        assert!(!passphrase_ok("12345678"));
        assert!(passphrase_ok("1234567a"));
        assert!(passphrase_ok("ééééééé1"));
        assert!(!passphrase_ok("abc"));
        assert_eq!(new_lock_hint("", ""), (format!("{RULE}."), false));
        assert_eq!(new_lock_hint("short", ""), (format!("{RULE}."), true));
        assert_eq!(new_lock_hint("long enough", "long"), ("The two do not match.".into(), true));
        assert_eq!(new_lock_hint("long enough", ""), (format!("{RULE}."), false));
        assert_eq!(new_lock_hint("long enough", "long enough"), (format!("{RULE}."), false));
    }

    #[::core::prelude::v1::test]
    fn the_add_form() {
        assert!(add_ok("N", "v", "open-sesame", "open-sesame"));
        assert!(!add_ok(" ", "v", "open-sesame", "open-sesame"));
        assert!(!add_ok("N", "", "open-sesame", "open-sesame"));
        assert!(!add_ok("N", "v", "open-sesame", "open-sesam"));
        assert!(!add_ok("N", "v", "12345678", "12345678"));
    }

    #[::core::prelude::v1::test]
    fn rows_read() {
        assert_eq!(req_meta(&json!({ "project": "demo", "reason": "deploy" })), "project: demo · deploy");
        assert_eq!(req_meta(&json!({})), "Claude requested access");
        assert_eq!(tag_of(&json!({ "needsUpgrade": true, "lock": "pin" })).0, "Old PIN");
        assert_eq!(tag_of(&json!({ "needsUpgrade": true, "lock": "passphrase" })).0, "Old lock");
        assert_eq!(tag_of(&json!({ "lock": "pin" })).0, "");
        assert_eq!(lock_of(&json!({ "lock": "passphrase" })), "passphrase");
        assert_eq!(lock_of(&json!({})), "pin");
        assert_eq!(ask_word("pin"), "PIN");
        assert_eq!(ask_word("passphrase"), "passphrase");
    }

    #[::core::prelude::v1::test]
    fn edit_rules_and_body() {
        let base = EditForm { name0: "A", desc0: "d", lock: "passphrase", name: "A", desc: "d", value: "", relock: false, new: "", new2: "", cur: "" };
        assert!(!base.ok(), "nothing changed");
        let meta = EditForm { name: "B ", ..base };
        assert!(meta.ok(), "a new name needs no lock");
        assert_eq!(meta.body("x"), json!({ "action": "updateMeta", "id": "x", "name": "B", "description": "d" }));
        let val = EditForm { value: "v2", ..base };
        assert!(!val.ok(), "a new value needs the lock");
        let val = EditForm { value: "v2", cur: "pass", ..base };
        assert!(val.ok());
        assert_eq!(val.body("x"), json!({ "action": "edit", "id": "x", "pin": "pass", "value": "v2" }));
        let re = EditForm { relock: true, new: "new passphrase", new2: "new passphrase", cur: "pass", ..base };
        assert!(re.ok());
        assert_eq!(re.body("x")["newPin"], json!("new passphrase"));
        let bad = EditForm { relock: true, new: "short", new2: "short", cur: "pass", ..base };
        assert!(!bad.ok());
        let pin = EditForm { lock: "pin", value: "v", cur: "12345", ..base };
        assert!(!pin.ok());
        let pin = EditForm { lock: "pin", value: "v", cur: "123456", ..base };
        assert!(pin.ok());
        let empty = EditForm { name: " ", ..base };
        assert!(!empty.ok());
    }

    #[::core::prelude::v1::test]
    fn answers() {
        assert_eq!(answer(Err("Not supported by this server".into()))["error"], json!(UNSUPPORTED));
        assert_eq!(answer(Ok(json!({ "ok": false, "error": "Unknown secrets action: edit" })))["error"], json!(UNSUPPORTED));
        assert_eq!(answer(Ok(json!({ "ok": false, "error": "Incorrect passphrase" })))["error"], json!("Incorrect passphrase"));
        assert_eq!(answer(Err("Cannot reach the server: x".into()))["ok"], json!(false));
        assert_eq!(answer(Ok(json!({ "ok": true, "value": "v" })))["value"], json!("v"));
        assert!(lockish("Incorrect passphrase"));
        assert!(lockish("PIN must be 6 digits"));
        assert!(!lockish("Name is taken"));
    }
}
