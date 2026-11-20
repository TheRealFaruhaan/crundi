//! The saved passwords' store (specs/desktop/passwords-homepool-cru.md A.1,
//! A.2; app/drawer-passwords.js createPasswordStore): local only, never on
//! the server. <config>/drawer-passwords.json, mode 0600, written through
//! .tmp + rename: { v: 1, logins: [{ id, origin, username, pw, created,
//! updated, used }], never: [origin…] }. Only the password is encrypted:
//! AES-256-GCM with a key the OS keychain keeps (passwords.rs), `pw` =
//! base64(nonce ‖ ciphertext). Pure: the key is given; unit-tested.

use aes_gcm::{aead::{Aead, AeadCore, KeyInit, OsRng}, Aes256Gcm, Key, Nonce};
use base64::Engine as _;
use serde_json::{json, Value};
use std::path::PathBuf;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// Scheme + host + port, http and https only (originOf).
pub fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" { return None; }
    let host = rest.split(['/', '?', '#']).next()?.rsplit('@').next()?.to_ascii_lowercase();
    if host.is_empty() { return None; }
    // The default port is left out, as URL.origin does.
    let host = match (scheme.as_str(), host.rsplit_once(':')) {
        ("http", Some((h, "80"))) | ("https", Some((h, "443"))) => h.to_string(),
        _ => host,
    };
    Some(format!("{scheme}://{host}"))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Login { pub id: String, pub origin: String, pub username: String, pub pw: String, pub created: u64, pub updated: u64, pub used: u64 }

pub struct Store {
    file: PathBuf,
    key: Option<[u8; 32]>,
    logins: Vec<Login>,
    never: Vec<String>,
    loaded: bool,
}

fn now() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0) }

fn uuid() -> String {
    let mut b = [0u8; 16];
    getrandom(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

/// Random bytes (the OS's, through aes-gcm's OsRng).
pub fn getrandom(out: &mut [u8]) {
    use aes_gcm::aead::rand_core::RngCore;
    OsRng.fill_bytes(out);
}

impl Store {
    /// The store in `file`; `key` None: no keychain (nothing can be saved or read).
    pub fn new(file: PathBuf, key: Option<[u8; 32]>) -> Store { Store { file, key, logins: vec![], never: vec![], loaded: false } }

    fn load(&mut self) {
        if self.loaded { return; }
        self.loaded = true;
        let v: Value = std::fs::read(&self.file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
        let s = |x: &Value, k: &str| x.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let n = |x: &Value, k: &str| x.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
        self.logins = v.get("logins").and_then(|l| l.as_array()).map(|a| a.iter().map(|l| Login {
            id: s(l, "id"), origin: s(l, "origin"), username: s(l, "username"), pw: s(l, "pw"), created: n(l, "created"), updated: n(l, "updated"), used: n(l, "used"),
        }).collect()).unwrap_or_default();
        self.never = v.get("never").and_then(|l| l.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    }

    fn persist(&self) -> Result<(), String> {
        let v = json!({ "v": 1, "never": self.never, "logins": self.logins.iter().map(|l| json!({
            "id": l.id, "origin": l.origin, "username": l.username, "pw": l.pw, "created": l.created, "updated": l.updated, "used": l.used })).collect::<Vec<_>>() });
        if let Some(d) = self.file.parent() { std::fs::create_dir_all(d).map_err(|e| e.to_string())?; }
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, v.to_string()).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)); }
        std::fs::rename(&tmp, &self.file).map_err(|e| e.to_string())
    }

    fn cipher(&self) -> Result<Aes256Gcm, String> {
        let k = self.key.ok_or_else(|| "Saved passwords are not available in this app.".to_string())?;
        Ok(Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&k)))
    }

    fn encrypt(&self, pw: &str) -> Result<String, String> {
        let c = self.cipher()?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ct = c.encrypt(&nonce, pw.as_bytes()).map_err(|_| "Could not encrypt".to_string())?;
        let mut out = nonce.to_vec();
        out.extend(ct);
        Ok(B64.encode(out))
    }

    fn decrypt(&self, pw: &str) -> Result<String, String> {
        let c = self.cipher()?;
        let b = B64.decode(pw).map_err(|_| "Could not read it".to_string())?;
        if b.len() < 13 { return Err("Could not read it".into()); }
        let (n, ct) = b.split_at(12);
        let p = c.decrypt(Nonce::from_slice(n), ct).map_err(|_| "Could not read it".to_string())?;
        String::from_utf8(p).map_err(|_| "Could not read it".into())
    }

    /// Every saved login (no passwords), or one site's; by origin, then username.
    pub fn list(&mut self, origin: Option<&str>) -> Vec<Login> {
        self.load();
        let mut v: Vec<Login> = self.logins.iter().filter(|l| origin.is_none_or(|o| l.origin == o)).cloned().collect();
        v.sort_by(|a, b| a.origin.cmp(&b.origin).then(a.username.cmp(&b.username)));
        v
    }

    pub fn count(&mut self, origin: &str) -> usize { self.load(); self.logins.iter().filter(|l| l.origin == origin).count() }

    /// What saving this would do: "new", "update", "same" or "never".
    pub fn check(&mut self, origin: &str, username: &str, password: &str) -> &'static str {
        self.load();
        if self.never.iter().any(|o| o == origin) { return "never"; }
        let Some(l) = self.logins.iter().find(|x| x.origin == origin && x.username == username) else { return "new" };
        if self.decrypt(&l.pw).is_ok_and(|p| p == password) { "same" } else { "update" }
    }

    pub fn save(&mut self, origin: &str, username: &str, password: &str) -> Result<Login, String> {
        self.cipher()?;
        if origin_of(origin).as_deref() != Some(origin) { return Err("not a site".into()); }
        if password.is_empty() { return Err("no password".into()); }
        self.load();
        let pw = self.encrypt(password)?;
        let t = now();
        let i = match self.logins.iter().position(|x| x.origin == origin && x.username == username) {
            Some(i) => { self.logins[i].pw = pw; self.logins[i].updated = t; i }
            None => { self.logins.push(Login { id: uuid(), origin: origin.into(), username: username.into(), pw, created: t, updated: t, used: 0 }); self.logins.len() - 1 }
        };
        self.never.retain(|o| o != origin);
        self.persist()?;
        Ok(self.logins[i].clone())
    }

    /// The password itself (to fill it, or to show it once the PIN was given).
    pub fn password(&mut self, id: &str) -> Result<(String, String, String), String> {
        self.cipher()?;
        self.load();
        let l = self.logins.iter().find(|x| x.id == id).ok_or("No such saved login")?;
        Ok((l.origin.clone(), l.username.clone(), self.decrypt(&l.pw)?))
    }

    pub fn used(&mut self, id: &str) {
        self.load();
        if let Some(l) = self.logins.iter_mut().find(|x| x.id == id) { l.used = now(); let _ = self.persist(); }
    }

    #[allow(dead_code)]
    pub fn update(&mut self, id: &str, username: Option<&str>, password: Option<&str>) -> Result<Login, String> {
        self.cipher()?;
        self.load();
        let i = self.logins.iter().position(|x| x.id == id).ok_or("No such saved login")?;
        if let Some(u) = username {
            let o = self.logins[i].origin.clone();
            if self.logins.iter().enumerate().any(|(j, x)| j != i && x.origin == o && x.username == u) { return Err("That site already has a login with this username".into()); }
            self.logins[i].username = u.to_string();
        }
        if let Some(p) = password.filter(|p| !p.is_empty()) { self.logins[i].pw = self.encrypt(p)?; }
        self.logins[i].updated = now();
        self.persist()?;
        Ok(self.logins[i].clone())
    }

    pub fn remove(&mut self, id: &str) -> bool {
        self.load();
        let n = self.logins.len();
        self.logins.retain(|x| x.id != id);
        let gone = self.logins.len() != n;
        if gone { let _ = self.persist(); }
        gone
    }

    pub fn never(&mut self, origin: &str) { self.load(); if !self.never.iter().any(|o| o == origin) { self.never.push(origin.into()); let _ = self.persist(); } }
    pub fn never_list(&mut self) -> Vec<String> { self.load(); let mut v = self.never.clone(); v.sort(); v }
    pub fn unnever(&mut self, origin: &str) { self.load(); self.never.retain(|o| o != origin); let _ = self.persist(); }
    /// Everything gone (Clear browsing data, "Saved passwords").
    pub fn clear(&mut self) { self.loaded = true; self.logins.clear(); self.never.clear(); let _ = self.persist(); }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str) -> Store {
        let f = std::env::temp_dir().join(format!("crundi-pw-test-{name}-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&f);
        Store::new(f, Some([7u8; 32]))
    }

    #[test]
    fn origins() {
        assert_eq!(origin_of("https://Example.com/login?x=1").as_deref(), Some("https://example.com"));
        assert_eq!(origin_of("http://a.b:8080/x").as_deref(), Some("http://a.b:8080"));
        assert_eq!(origin_of("https://a.b:443/").as_deref(), Some("https://a.b"));
        assert_eq!(origin_of("about:blank"), None);
        assert_eq!(origin_of("file:///etc"), None);
    }

    #[test]
    fn save_check_update_and_read() {
        let mut s = store("a");
        assert_eq!(s.check("https://a.b", "me", "pw1"), "new");
        let l = s.save("https://a.b", "me", "pw1").unwrap();
        assert_ne!(l.pw, "pw1");
        assert_eq!(s.check("https://a.b", "me", "pw1"), "same");
        assert_eq!(s.check("https://a.b", "me", "pw2"), "update");
        s.save("https://a.b", "me", "pw2").unwrap();
        assert_eq!(s.count("https://a.b"), 1);
        assert_eq!(s.password(&l.id).unwrap().2, "pw2");
        // Read back from the file, by a new store with the same key.
        let mut t = Store::new(s.file.clone(), Some([7u8; 32]));
        assert_eq!(t.password(&l.id).unwrap().2, "pw2");
        // Another key reads nothing.
        let mut u = Store::new(s.file.clone(), Some([8u8; 32]));
        assert!(u.password(&l.id).is_err());
        assert_eq!(s.save("about:blank", "x", "y").unwrap_err(), "not a site");
        assert_eq!(s.save("https://a.b", "x", "").unwrap_err(), "no password");
        s.save("https://a.b", "other", "p").unwrap();
        assert_eq!(s.update(&l.id, Some("other"), None).unwrap_err(), "That site already has a login with this username");
        assert_eq!(s.list(None).iter().map(|l| l.username.as_str()).collect::<Vec<_>>(), ["me", "other"]);
        assert!(s.remove(&l.id));
        assert_eq!(s.password(&l.id).unwrap_err(), "No such saved login");
    }

    #[test]
    fn never_and_clear() {
        let mut s = store("b");
        s.never("https://x.y");
        assert_eq!(s.check("https://x.y", "a", "b"), "never");
        s.save("https://x.y", "a", "b").unwrap();
        assert!(s.never_list().is_empty());
        s.never("https://z.y");
        s.unnever("https://z.y");
        assert!(s.never_list().is_empty());
        s.clear();
        assert!(s.list(None).is_empty());
    }

    #[test]
    fn no_key_no_saving() {
        let f = std::env::temp_dir().join(format!("crundi-pw-test-c-{}.json", std::process::id()));
        let mut s = Store::new(f, None);
        assert!(s.save("https://a.b", "me", "x").is_err());
    }
}
