//! The Crundi server's HTTP API, as the PC client uses it.
//!
//! Access tokens last 15 minutes and refresh tokens 30 days; a refresh hands
//! back a new pair and spends the old refresh token, and spending one twice
//! (outside a short grace) signs the whole login out. So refreshes are one at
//! a time ([Api::refresh_after]) and every 401 is answered by exactly one
//! refresh and one retry, as the web UI does. Blocking calls: run them off the
//! UI thread.

use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// How the server recognises this client (src/client-version.js). Not sent
/// as "desktop" yet: that would hold the prototype to the Electron client's
/// minimum version.
pub const USER_AGENT: &str = concat!("CrundiNative/", env!("CARGO_PKG_VERSION"));

#[derive(Debug)]
pub enum ApiError {
    /// The login is gone (refresh refused): sign in again.
    SignedOut,
    /// The server answered with an error: its status and words.
    Status(u16, String),
    /// Could not reach the server.
    Net(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::SignedOut => write!(f, "Signed out: sign in again"),
            ApiError::Status(404, _) => write!(f, "Not supported by this server"),
            ApiError::Status(code, msg) if msg.is_empty() => write!(f, "The server answered {code}"),
            ApiError::Status(_, msg) => write!(f, "{msg}"),
            ApiError::Net(e) => write!(f, "Cannot reach the server: {e}"),
        }
    }
}

pub type ApiResult<T = Value> = Result<T, ApiError>;

#[derive(Clone, Debug)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    pub expires_at: Instant,
}

impl Tokens {
    /// From a login or refresh answer: {token, refreshToken, expiresIn (s)}.
    pub fn from_answer(v: &Value) -> Option<Tokens> {
        let access = v.get("token")?.as_str()?.to_string();
        let refresh = v.get("refreshToken")?.as_str()?.to_string();
        let secs = v.get("expiresIn").and_then(|e| e.as_u64()).unwrap_or(900);
        Some(Tokens { access, refresh, expires_at: Instant::now() + Duration::from_secs(secs) })
    }
}

fn agent(timeout: Option<Duration>) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(timeout)
        .user_agent(USER_AGENT)
        .build()
        .into()
}

/// The server's base URL, tidied: a scheme, no trailing slash, no path.
pub fn normalize_base(input: &str) -> String {
    let t = input.trim().trim_end_matches('/');
    let with_scheme = if t.contains("://") { t.to_string() } else if t.starts_with("localhost") || t.starts_with("127.") { format!("http://{t}") } else { format!("https://{t}") };
    // Keep scheme://host[:port] only.
    match with_scheme.find("://").map(|i| i + 3) {
        Some(start) => match with_scheme[start..].find('/') {
            Some(slash) => with_scheme[..start + slash].to_string(),
            None => with_scheme,
        },
        None => with_scheme,
    }
}

fn read(resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> (u16, ApiResult) {
    match resp {
        Ok(mut r) => {
            let status = r.status().as_u16();
            let body = r.body_mut().with_config().limit(256 * 1024 * 1024).read_to_string().unwrap_or_default();
            let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            if (200..300).contains(&status) {
                (status, Ok(v))
            } else {
                let msg = v.get("error").and_then(|e| e.as_str()).unwrap_or("").to_string();
                (status, Err(ApiError::Status(status, msg)))
            }
        }
        Err(e) => (0, Err(ApiError::Net(e.to_string()))),
    }
}

/// Unauthenticated: GET or POST on a server with no session.
pub fn open_call(base: &str, method: &str, path: &str, body: Option<&Value>) -> ApiResult {
    let a = agent(Some(Duration::from_secs(20)));
    let url = format!("{base}{path}");
    let r = match (method, body) {
        ("GET", _) => a.get(&url).call(),
        (_, Some(b)) => a.post(&url).header("Content-Type", "application/json").send(b.to_string()),
        (_, None) => a.post(&url).send_empty(),
    };
    read(r).1
}

// ─── Signing in ───

/// What the server offers: {telegram, password, google, github, setupRequired}.
pub fn methods(base: &str) -> ApiResult {
    open_call(base, "GET", "/api/auth/methods", None)
}

/// The server's version and what it can do (no auth).
pub fn handshake(base: &str) -> ApiResult {
    open_call(base, "GET", "/api/handshake", None)
}

pub struct SignedIn {
    pub base: String,
    pub tokens: Tokens,
    pub username: String,
}

/// Password and the authenticator's 6-digit code.
pub fn password_login(base: &str, password: &str, code: &str) -> ApiResult<SignedIn> {
    let v = open_call(base, "POST", "/api/auth/password", Some(&json!({ "password": password, "code": code.trim() })))?;
    let tokens = Tokens::from_answer(&v).ok_or(ApiError::Status(0, "The server sent no session".into()))?;
    Ok(SignedIn { base: base.to_string(), tokens, username: String::new() })
}

/// A pairing link from a signed-in device: "<server>/pair#<code>". Claims it
/// for this computer and returns its own login (the "page" session).
pub fn pair_claim(link: &str) -> ApiResult<SignedIn> {
    let (base, code) = split_pair_link(link).ok_or(ApiError::Status(0, "That is not a pairing link (it looks like https://…/pair#…)".into()))?;
    let name = gethostname::gethostname().to_string_lossy().to_string();
    let v = open_call(&base, "POST", "/api/pair/claim", Some(&json!({ "code": code, "device": { "name": name, "platform": "desktop" } })))?;
    let tokens = v.pointer("/sessions/page").and_then(Tokens::from_answer).ok_or(ApiError::Status(0, "The server sent no session".into()))?;
    let username = v.get("username").and_then(|u| u.as_str()).unwrap_or("").to_string();
    Ok(SignedIn { base, tokens, username })
}

pub fn split_pair_link(link: &str) -> Option<(String, String)> {
    let link = link.trim();
    let (url, code) = link.split_once('#')?;
    let code = code.trim().to_string();
    let base = url.trim_end_matches('/').strip_suffix("/pair")?;
    if code.len() < 8 { return None; }
    Some((normalize_base(base), code))
}

/// Google or GitHub, in the system browser: start it, then [oauth_poll] until done.
/// Answers {url, pollId, confirmCode}: the browser page asks the person to
/// check that code matches the one this client shows.
pub fn oauth_start(base: &str, provider: &str) -> ApiResult {
    open_call(base, "POST", "/api/auth/oauth/start", Some(&json!({ "provider": provider, "origin": base, "desktop": true })))
}

/// {status: waiting|confirm|failed|expired} or {status: done, token, refreshToken, expiresIn}.
pub fn oauth_poll(base: &str, poll_id: &str) -> ApiResult {
    open_call(base, "POST", "/api/auth/oauth/poll", Some(&json!({ "pollId": poll_id })))
}

/// Signing in through the browser (the server's src/app-login.js): a
/// pending login for this computer. Answers {url, pollId, confirmCode}: open
/// url in the browser, where the person signs in with any method the server
/// offers and allows this app; then [app_login_poll] collects the login.
pub fn app_login_start(base: &str) -> ApiResult {
    let name = gethostname::gethostname().to_string_lossy().to_string();
    open_call(base, "POST", "/api/auth/app/start", Some(&json!({ "origin": base, "device": { "name": name, "platform": "desktop" } })))
}

/// {status: waiting|denied|expired}, or {status: approved, username, sessions: {page, background}}.
pub fn app_login_poll(base: &str, poll_id: &str) -> ApiResult {
    open_call(base, "POST", "/api/auth/app/poll", Some(&json!({ "pollId": poll_id })))
}

/// A saved login: a new access token (and refresh token) from the refresh token.
pub fn refresh(base: &str, refresh_token: &str) -> ApiResult<Tokens> {
    match open_call(base, "POST", "/api/auth/refresh", Some(&json!({ "refreshToken": refresh_token }))) {
        Ok(v) => Tokens::from_answer(&v).ok_or(ApiError::SignedOut),
        Err(ApiError::Status(401, _)) | Err(ApiError::Status(403, _)) => Err(ApiError::SignedOut),
        Err(e) => Err(e),
    }
}

// ─── A signed-in session ───

struct Inner {
    base: String,
    agent: ureq::Agent,
    tokens: Mutex<Tokens>,
    /// One refresh at a time (a refresh token is spent by its use).
    refresh_lock: Mutex<()>,
    /// Told about every new pair of tokens (to save the refresh token).
    on_tokens: Box<dyn Fn(&Tokens) + Send + Sync>,
}

#[derive(Clone)]
pub struct Api(Arc<Inner>);

impl Api {
    pub fn new(base: &str, tokens: Tokens, on_tokens: impl Fn(&Tokens) + Send + Sync + 'static) -> Api {
        Api(Arc::new(Inner {
            base: base.to_string(),
            agent: agent(Some(Duration::from_secs(60))),
            tokens: Mutex::new(tokens),
            refresh_lock: Mutex::new(()),
            on_tokens: Box::new(on_tokens),
        }))
    }

    pub fn base(&self) -> &str {
        &self.0.base
    }

    pub fn access(&self) -> String {
        self.0.tokens.lock().unwrap().access.clone()
    }

    /// A fresh access token if this one is near its end (for a socket about to connect).
    pub fn fresh_access(&self) -> ApiResult<String> {
        let (access, near_end) = {
            let t = self.0.tokens.lock().unwrap();
            (t.access.clone(), t.expires_at.saturating_duration_since(Instant::now()) < Duration::from_secs(120))
        };
        if near_end { self.refresh_after(&access)?; }
        Ok(self.access())
    }

    /// Refresh, unless someone already did since [used] was handed out.
    pub fn refresh_after(&self, used: &str) -> ApiResult<()> {
        let _one_at_a_time = self.0.refresh_lock.lock().unwrap();
        let refresh_token = {
            let t = self.0.tokens.lock().unwrap();
            if t.access != used { return Ok(()); }
            t.refresh.clone()
        };
        let fresh = refresh(&self.0.base, &refresh_token)?;
        *self.0.tokens.lock().unwrap() = fresh.clone();
        (self.0.on_tokens)(&fresh);
        Ok(())
    }

    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> ApiResult {
        let url = format!("{}{}", self.0.base, path);
        let mut access = self.access();
        for attempt in 0..2 {
            let req = |token: &str| {
                let auth = format!("Bearer {token}");
                match (method, body) {
                    ("GET", _) => self.0.agent.get(&url).header("Authorization", &auth).call(),
                    (_, Some(b)) => self.0.agent.post(&url).header("Authorization", &auth).header("Content-Type", "application/json").send(b.to_string()),
                    (_, None) => self.0.agent.post(&url).header("Authorization", &auth).send_empty(),
                }
            };
            let (status, result) = read(req(&access));
            if status == 401 && attempt == 0 {
                self.refresh_after(&access)?;
                access = self.access();
                continue;
            }
            if status == 401 { return Err(ApiError::SignedOut); }
            return result;
        }
        unreachable!()
    }

    pub fn get(&self, path: &str) -> ApiResult {
        self.call("GET", path, None)
    }

    pub fn post(&self, path: &str, body: &Value) -> ApiResult {
        self.call("POST", path, Some(body))
    }

    /// The URL of the server's /ws, with a fresh access token.
    pub fn ws_url(&self) -> ApiResult<String> {
        let token = self.fresh_access()?;
        let ws_base = self.0.base.replacen("https://", "wss://", 1).replacen("http://", "ws://", 1);
        Ok(format!("{ws_base}/ws?token={token}"))
    }

    /// The server's event stream (/api/events): calls [on_event](name, data)
    /// for each event until the stream ends or fails. Blocks.
    pub fn events(&self, mut on_event: impl FnMut(&str, &str)) -> ApiResult<()> {
        use std::io::BufRead;
        let token = self.fresh_access()?;
        let url = format!("{}/api/events?token={}", self.0.base, token);
        let a = agent(None);
        let resp = a.get(&url).header("Accept", "text/event-stream").call().map_err(|e| ApiError::Net(e.to_string()))?;
        let status = resp.status().as_u16();
        if status == 401 {
            self.refresh_after(&token)?;
            return Err(ApiError::Status(401, String::new()));
        }
        if status != 200 { return Err(ApiError::Status(status, String::new())); }
        let reader = std::io::BufReader::new(resp.into_body().into_reader());
        let (mut name, mut data) = (String::new(), String::new());
        for line in reader.lines() {
            let line = line.map_err(|e| ApiError::Net(e.to_string()))?;
            if line.is_empty() {
                if !data.is_empty() { on_event(if name.is_empty() { "message" } else { &name }, &data); }
                name.clear();
                data.clear();
            } else if let Some(v) = line.strip_prefix("event:") {
                name = v.trim().to_string();
            } else if let Some(v) = line.strip_prefix("data:") {
                if !data.is_empty() { data.push('\n'); }
                data.push_str(v.strip_prefix(' ').unwrap_or(v));
            }
            // ":" lines (padding, keep-alives) are ignored.
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bases_and_pair_links() {
        assert_eq!(normalize_base("dev.faruhaan.com/"), "https://dev.faruhaan.com");
        assert_eq!(normalize_base("localhost:8889"), "http://localhost:8889");
        assert_eq!(normalize_base("https://x.y/some/path"), "https://x.y");
        let (b, c) = split_pair_link("https://dev.faruhaan.com/pair#0123456789abcdef01234567").unwrap();
        assert_eq!((b.as_str(), c.as_str()), ("https://dev.faruhaan.com", "0123456789abcdef01234567"));
        assert!(split_pair_link("https://x/notpair#abc").is_none());
    }
}
