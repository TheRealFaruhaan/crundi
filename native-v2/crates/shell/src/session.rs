//! The session store (an entity, the shell's): the server, the API, the
//! live link, the connection, and sign-in's own state. Sign-in, resume and
//! sign-out follow native-gpui's session.rs (the Slint client's app.rs);
//! server calls run off the UI thread. It notifies only itself; the views
//! that show it observe it (ARCHITECTURE.md rules 1-2). Signing in or out is
//! also an event (`SessionEvent`), which the shell's init turns into the
//! live hub, the projects and the areas' hooks.

use crundi_core::{api::{self, Api, ApiError, SignedIn}, live::{self, Event, Live, Stats}, store};
use gpui::*;
use serde_json::Value;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};

/// Where sign-in is: which server, then signing in to it; `Boot` while a
/// saved login is being resumed (the web's start-up screen, #boot: the
/// sign-in form never flashes for a saved login).
#[derive(Clone, Default, PartialEq, Debug)]
pub enum Step { #[default] Server, Login, Boot }

#[derive(Default)]
pub struct Session {
    pub api: Option<Api>,
    live: Option<Live>,
    pub online: bool,
    pub server: String,
    pub username: String,
    // Sign-in.
    pub step: Step,
    pub signin_server: String,
    pub signin_error: String,
    pub signin_busy: bool,
    pub offer_password: bool,
    pub offer_google: bool,
    pub offer_github: bool,
    pub offer_telegram: bool,
    pub app_login: bool,
    /// Signing in through the browser: the code to compare with the page's ("" none).
    pub oauth_code: String,
    oauth_cancel: Option<Arc<AtomicBool>>,
    /// The project to open after sign-in (the saved one).
    pub want_project: String,
    /// The start-up screen has been up a while: its note says so (1.5 s).
    pub boot_slow: bool,
    /// The server has no sign-in set up yet (methods' setupRequired): the
    /// first-run form (set a password, or restore from a backup).
    pub setup_required: bool,
    /// The password was just set: the authenticator's secret and its
    /// otpauth address, shown once ("One more step"); the session waits in
    /// `pending` until Continue.
    pub enrol: Option<(String, String)>,
    pending: Option<SignedIn>,
    /// The collaborator's way in is shown instead of the owner's ("I was
    /// invited to a project").
    pub collab: bool,
    /// The server step shown over the signed-in app (the tray's "Server…",
    /// Electron's showServerScreen; F131): Cancel goes back, Save keeps the
    /// address for the next start.
    pub server_over: bool,
    /// The server step's note while over the app (Electron's #srv-note).
    pub server_note: String,
    /// Failed password sign-ins (and collaborator ones): the view clears the code (or passcode).
    pub failed_logins: u64,
    /// Local mode runs this install's own server (crundi_shell::local): the
    /// saved login is not resumed at start; the server's key signs in once
    /// it is ready (`sign_in_local`).
    pub hold: bool,
    /// The live link's channel, until the hub takes it.
    events: Option<futures::channel::mpsc::UnboundedReceiver<Event>>,
}

/// Signed in (the live link's channel waits in `take_events`) or out.
pub enum SessionEvent { SignedIn, SignedOut, ServerScreen }
impl EventEmitter<SessionEvent> for Session {}

impl Session {
    pub fn signed_in(&self) -> bool { self.api.is_some() }

    /// The live link's counters (None while signed out).
    pub fn stats(&self) -> Option<Arc<Stats>> { self.live.as_ref().map(|l| l.stats.clone()) }

    pub fn live(&self) -> Option<&Live> { self.live.as_ref() }

    /// The live link's channel (once per sign-in: the hub's).
    pub fn take_events(&mut self) -> Option<futures::channel::mpsc::UnboundedReceiver<Event>> { self.events.take() }

    /// The saved login, picked up (called once, at start).
    pub fn start(&mut self, cx: &mut Context<Self>) {
        if self.hold { return; }
        let saved = store::load();
        self.signin_server = host_of(&saved.server);
        if saved.server.is_empty() {
            self.step = Step::Server;
        } else if !saved.refresh_token.is_empty() {
            self.resume(saved, cx);
        } else {
            let s = saved.server.clone();
            self.check_server(&s, cx);
        }
        cx.notify();
    }

    fn busy(&mut self, on: bool, error: &str, cx: &mut Context<Self>) {
        self.signin_busy = on;
        self.signin_error = error.to_string();
        cx.notify();
    }

    /// Blocking work off the UI thread, then `done` on it with this store.
    fn run<T: Send + 'static>(cx: &mut Context<Self>, work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static) {
        let task = cx.background_spawn(async move { work() });
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let r = task.await;
            let _ = this.update(cx, |s, cx| done(s, r, cx));
        }).detach();
    }

    fn resume(&mut self, saved: store::Saved, cx: &mut Context<Self>) {
        // The start-up screen until it is known which to show; slow, it says so.
        self.step = Step::Boot;
        self.boot_slow = false;
        let t = cx.background_executor().timer(std::time::Duration::from_millis(1500));
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            t.await;
            let _ = this.update(cx, |s, cx| if s.step == Step::Boot && !s.boot_slow { s.boot_slow = true; cx.notify(); });
        }).detach();
        self.busy(true, "", cx);
        let (base, token, username) = (saved.server.clone(), saved.refresh_token.clone(), saved.username.clone());
        Self::run(cx, move || api::refresh(&base, &token).map(|tokens| SignedIn { base, tokens, username }), |s, r, cx| match r {
            Ok(si) => { s.busy(false, "", cx); s.start_session(si, cx); }
            Err(_) => {
                store::update(|s| s.refresh_token.clear());
                s.busy(false, "", cx);
                let srv = store::load().server;
                s.check_server(&srv, cx);
            }
        });
    }

    /// The server step: what this server offers for signing in.
    pub fn check_server(&mut self, input: &str, cx: &mut Context<Self>) {
        let base = api::normalize_base(input);
        let saved = store::load();
        if saved.server == base && !saved.refresh_token.is_empty() { return self.resume(saved, cx); }
        self.busy(true, "", cx);
        Self::run(cx, move || {
            let m = api::methods(&base)?;
            let caps = api::handshake(&base).ok().and_then(|h| h.pointer("/server/capabilities").cloned()).unwrap_or(Value::Null);
            let app_login = caps.as_array().map(|c| c.iter().any(|x| x == "app-login")).unwrap_or(false);
            Ok::<_, ApiError>((base, m, app_login))
        }, |s, r, cx| {
            s.busy(false, "", cx);
            match r {
                Ok((base, m, app_login)) => {
                    let on = |k: &str| m.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
                    s.app_login = app_login;
                    s.offer_telegram = on("telegram");
                    s.signin_server = host_of(&base);
                    s.offer_password = on("password");
                    s.offer_google = on("google");
                    s.offer_github = on("github");
                    s.setup_required = on("setupRequired");
                    s.step = Step::Login;
                    s.collab = false;
                    s.enrol = None;
                    store::update(|st| { if st.server != base { st.refresh_token.clear(); } st.server = base.clone(); });
                }
                Err(e) => {
                    // A saved server that cannot be reached: its step, with why.
                    if s.step == Step::Boot { s.step = Step::Server; }
                    s.signin_error = e.to_string();
                }
            }
            cx.notify();
        });
    }

    /// The server step over the signed-in app, or back to the app (F131).
    pub fn show_server_over(&mut self, on: bool, cx: &mut Context<Self>) {
        if !self.signed_in() && on { return self.change_server(cx); }
        self.server_over = on;
        self.server_note = if on && self.online { "Connected \u{2014} the browser panel is being served from this machine.".into() } else if on { "Not currently connected.".into() } else { String::new() };
        cx.emit(SessionEvent::ServerScreen);
        cx.notify();
    }

    /// Save, over the app: the address is kept for the next start (Electron's
    /// client:setConfig: the window keeps what it is connected to until then).
    pub fn save_server_over(&mut self, input: &str, cx: &mut Context<Self>) {
        let t = input.trim();
        let lower = t.to_ascii_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            self.server_note = "Enter the full server address, including http:// or https://".into();
        } else {
            let base = api::normalize_base(t);
            store::update(|st| { if st.server != base { st.refresh_token.clear(); } st.server = base.clone(); });
            self.server_note = "Saved. Restart Crundi for it to take effect.".into();
        }
        cx.notify();
    }

    /// The local server is ready (Electron's load of
    /// `http://localhost:<port>?key=…`): its key for a session, as the web's
    /// tryLocalAuth (POST /api/auth/local).
    pub fn sign_in_local(&mut self, base: String, key: String, cx: &mut Context<Self>) {
        self.busy(true, "", cx);
        Self::run(cx, move || {
            let v = api::open_call(&base, "POST", "/api/auth/local", Some(&serde_json::json!({ "key": key })))?;
            let tokens = api::Tokens::from_answer(&v).ok_or(ApiError::Status(0, "The server sent no session".into()))?;
            Ok(SignedIn { base, tokens, username: String::new() })
        }, |s, r, cx| s.finish_sign_in(r, cx));
    }

    /// The local server stopped (Electron's loadFile(index.html)): out of the
    /// app, quietly (no logout: the server is gone), to the status screen.
    pub fn end_local(&mut self, cx: &mut Context<Self>) {
        if self.api.is_none() { return; }
        self.sign_out("The server stopped", cx);
        self.signin_error.clear();
        cx.notify();
    }

    /// Back to the server step (the login step's "Change").
    pub fn change_server(&mut self, cx: &mut Context<Self>) {
        self.step = Step::Server;
        self.signin_error.clear();
        cx.notify();
    }

    /// Sign in with the password and the authenticator's code.
    pub fn sign_in_password(&mut self, server: &str, password: &str, code: &str, cx: &mut Context<Self>) {
        let (base, password, code) = (api::normalize_base(server), password.to_string(), code.to_string());
        self.busy(true, "", cx);
        Self::run(cx, move || api::password_login(&base, &password, &code), |s, r, cx| {
            // Codes are single-use in practice: a failed one is cleared (the view).
            if r.is_err() { s.failed_logins += 1; }
            s.finish_sign_in(r, cx)
        });
    }

    /// The server being signed in to (its base URL).
    pub fn signin_base(&self) -> String { api::normalize_base(&self.signin_server) }

    /// The collaborator's way in, or back to the owner's.
    pub fn show_collab(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.collab != on { self.collab = on; self.signin_error.clear(); cx.notify(); }
    }

    /// Sign in as an invited collaborator (POST /api/auth/collab: name and passcode).
    pub fn sign_in_collab(&mut self, name: &str, passcode: &str, cx: &mut Context<Self>) {
        let base = self.signin_base();
        let body = serde_json::json!({ "name": name.trim(), "passcode": passcode });
        self.busy(true, "", cx);
        Self::run(cx, move || {
            let v = api::open_call(&base, "POST", "/api/auth/collab", Some(&body)).map_err(|e| web_error(e, "Sign-in failed"))?;
            let tokens = api::Tokens::from_answer(&v).ok_or_else(|| "Sign-in failed".to_string())?;
            Ok(SignedIn { base, tokens, username: String::new() })
        }, |s, r: Result<SignedIn, String>, cx| match r {
            Ok(si) => { s.busy(false, "", cx); s.start_session(si, cx); }
            Err(e) => { s.failed_logins += 1; s.busy(false, &e, cx) }
        });
    }

    /// First run: set the password (POST /api/auth/setup). The server signs
    /// this app in with it; the authenticator's secret is shown before the
    /// app opens (`enrol`, then `finish_enrol`).
    pub fn set_up_password(&mut self, password: &str, cx: &mut Context<Self>) {
        let base = self.signin_base();
        let body = serde_json::json!({ "method": "password", "password": password });
        self.busy(true, "", cx);
        Self::run(cx, move || {
            let v = api::open_call(&base, "POST", "/api/auth/setup", Some(&body)).map_err(|e| web_error(e, "Could not set that"))?;
            if v.get("ok").and_then(|o| o.as_bool()) == Some(false) { let e = st(&v, "error"); return Err(if e.is_empty() { "Could not set that".to_string() } else { e }); }
            let tokens = api::Tokens::from_answer(&v).ok_or_else(|| "Could not set that".to_string())?;
            Ok((SignedIn { base, tokens, username: String::new() }, st(&v, "totpSecret"), st(&v, "uri")))
        }, |s, r: Result<(SignedIn, String, String), String>, cx| match r {
            Ok((si, secret, uri)) => { s.pending = Some(si); s.enrol = Some((secret, uri)); s.setup_required = false; s.busy(false, "", cx); }
            Err(e) => s.busy(false, &e, cx),
        });
    }

    /// "I have added it": into the app with the session the setup made.
    pub fn finish_enrol(&mut self, cx: &mut Context<Self>) {
        self.enrol = None;
        if let Some(si) = self.pending.take() { self.start_session(si, cx); }
        cx.notify();
    }

    /// Sign in with Google, GitHub or Telegram (the login step's buttons):
    /// through the server's own page when it signs apps in (app-login), else
    /// its OAuth flow.
    pub fn sign_in_oauth(&mut self, provider: &str, cx: &mut Context<Self>) {
        let base = api::normalize_base(&self.signin_server);
        if provider == "telegram" && !self.app_login {
            return self.busy(false, "This server can't sign this app in with Telegram yet: it needs a newer Crundi (one with app sign-in). Sign in with a password, Google or GitHub, or pair this app from the web.", cx);
        }
        if self.app_login || provider == "telegram" { return self.sign_in_browser(base, cx); }
        self.busy(true, "", cx);
        let cancel = self.new_cancel();
        let provider = provider.to_string();
        Self::run(cx, move || api::oauth_start(&base, &provider).map(|v| (base, v)), move |s, r, cx| {
            let (base, v) = match r { Ok(x) => x, Err(e) => return s.busy(false, &e.to_string(), cx) };
            let (url, poll_id, code) = (st(&v, "url"), st(&v, "pollId"), st(&v, "confirmCode"));
            if url.is_empty() || poll_id.is_empty() { return s.busy(false, "The server did not start the sign-in", cx); }
            if let Err(e) = open_url(&url) { return s.busy(false, &format!("Could not open the browser: {e}"), cx); }
            s.oauth_code = if code.is_empty() { "…".into() } else { code };
            s.signin_busy = false;
            cx.notify();
            Self::run(cx, move || {
                for _ in 0..300 {
                    if cancel.load(Ordering::Relaxed) { return Err(ApiError::Status(0, String::new())); }
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    match api::oauth_poll(&base, &poll_id) {
                        Ok(v) => match st(&v, "status").as_str() {
                            "done" => return api::Tokens::from_answer(&v).map(|t| SignedIn { base: base.clone(), tokens: t, username: String::new() }).ok_or(ApiError::Status(0, "The server sent no session".into())),
                            "failed" | "expired" => return Err(ApiError::Status(0, format!("Sign-in {}", st(&v, "status")))),
                            _ => {}
                        },
                        Err(ApiError::Net(_)) => {}
                        Err(e) => return Err(e),
                    }
                }
                Err(ApiError::Status(0, "Sign-in timed out".into()))
            }, Self::browser_done);
        });
    }

    /// Through the browser: the server's own web page, where the person signs
    /// in with any method it offers and allows this computer.
    fn sign_in_browser(&mut self, base: String, cx: &mut Context<Self>) {
        self.busy(true, "", cx);
        let cancel = self.new_cancel();
        Self::run(cx, move || api::app_login_start(&base).map(|v| (base, v)), move |s, r, cx| {
            let (base, v) = match r { Ok(x) => x, Err(e) => return s.busy(false, &e.to_string(), cx) };
            let (url, poll_id, code) = (st(&v, "url"), st(&v, "pollId"), st(&v, "confirmCode"));
            if url.is_empty() || poll_id.is_empty() { return s.busy(false, "The server did not start the sign-in", cx); }
            if let Err(e) = open_url(&url) { return s.busy(false, &format!("Could not open the browser: {e}"), cx); }
            s.oauth_code = code;
            s.signin_busy = false;
            cx.notify();
            Self::run(cx, move || {
                for _ in 0..300 {
                    if cancel.load(Ordering::Relaxed) { return Err(ApiError::Status(0, String::new())); }
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    match api::app_login_poll(&base, &poll_id) {
                        Ok(v) => match st(&v, "status").as_str() {
                            "approved" => {
                                let tokens = v.pointer("/sessions/page").and_then(api::Tokens::from_answer).ok_or(ApiError::Status(0, "The server sent no session".into()))?;
                                return Ok(SignedIn { base: base.clone(), tokens, username: st(&v, "username") });
                            }
                            "denied" => return Err(ApiError::Status(0, "The sign-in was denied in the browser.".into())),
                            "expired" => return Err(ApiError::Status(0, "The sign-in request expired. Try again.".into())),
                            _ => {}
                        },
                        Err(ApiError::Net(_)) => {}
                        Err(e) => return Err(e),
                    }
                }
                Err(ApiError::Status(0, "The sign-in request expired. Try again.".into()))
            }, Self::browser_done);
        });
    }

    /// Stop waiting for the browser (Cancel on the code step).
    pub fn cancel_browser(&mut self, cx: &mut Context<Self>) {
        if let Some(c) = self.oauth_cancel.take() { c.store(true, Ordering::Relaxed); }
        self.oauth_code.clear();
        self.signin_busy = false;
        cx.notify();
    }

    fn new_cancel(&mut self) -> Arc<AtomicBool> {
        let c = Arc::new(AtomicBool::new(false));
        if let Some(old) = self.oauth_cancel.replace(c.clone()) { old.store(true, Ordering::Relaxed); }
        c
    }

    fn browser_done(&mut self, r: Result<SignedIn, ApiError>, cx: &mut Context<Self>) {
        self.oauth_code.clear();
        self.oauth_cancel = None;
        match r {
            Err(ApiError::Status(0, m)) if m.is_empty() => self.busy(false, "", cx),
            r => self.finish_sign_in(r, cx),
        }
    }

    fn finish_sign_in(&mut self, r: Result<SignedIn, ApiError>, cx: &mut Context<Self>) {
        match r {
            Ok(si) => { self.busy(false, "", cx); self.start_session(si, cx); }
            Err(e) => self.busy(false, &web_error(e, "Sign-in failed"), cx),
        }
    }

    fn start_session(&mut self, si: SignedIn, cx: &mut Context<Self>) {
        let SignedIn { base, tokens, username } = si;
        let saved = store::load();
        let username = if username.is_empty() && saved.server == base { saved.username.clone() } else { username };
        store::update(|s| {
            if s.server != base { s.project.clear(); s.layouts.clear(); }
            s.server = base.clone();
            s.refresh_token = tokens.refresh.clone();
            s.username = username.clone();
        });
        let api = Api::new(&base, tokens, |t| store::update(|s| s.refresh_token = t.refresh.clone()));
        // The live link's one channel to the UI thread (the hub takes what is
        // waiting once per frame).
        let (tx, rx) = futures::channel::mpsc::unbounded();
        let live = live::start(api.clone(), move |e| { let _ = tx.unbounded_send(e); });
        if let Some(old) = self.live.replace(live) { old.stop(); }
        self.api = Some(api);
        self.server = host_of(&base);
        self.username = username;
        self.want_project = saved.project;
        self.events = Some(rx);
        cx.emit(SessionEvent::SignedIn);
        cx.notify();
    }

    /// Sign out (why: "" for the person's own choice, which also tells the server).
    pub fn sign_out(&mut self, why: &str, cx: &mut Context<Self>) {
        if self.api.is_none() { return; }
        let (api, live) = (self.api.take(), self.live.take());
        if let Some(l) = live { l.stop(); }
        self.events = None;
        if let (Some(api), true) = (api, why.is_empty()) {
            std::thread::spawn(move || { let _ = api.post("/api/auth/logout", &serde_json::json!({})); });
        }
        store::update(|s| s.refresh_token.clear());
        self.online = false;
        self.signin_error = why.to_string();
        self.step = Step::Login;
        self.collab = false;
        self.enrol = None;
        // What the server offers now (the web asks again on its login page:
        // a first-run server has a password by now).
        let base = self.signin_base();
        Self::run(cx, move || api::methods(&base), |s, r, cx| if let Ok(m) = r {
            let on = |k: &str| m.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
            (s.offer_telegram, s.offer_password, s.offer_google, s.offer_github, s.setup_required) = (on("telegram"), on("password"), on("google"), on("github"), on("setupRequired"));
            cx.notify();
        });
        cx.emit(SessionEvent::SignedOut);
        cx.notify();
    }

    /// The live link is up or down (the connection badge).
    pub fn set_online(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.online != on { self.online = on; cx.notify(); }
    }
}

pub fn host_of(base: &str) -> String { base.split("://").nth(1).unwrap_or(base).to_string() }

/// Open the browser at a page; tests (no desktop browser) get the address
/// in CRUNDI_OPEN_URL_FILE instead.
pub fn open_url(url: &str) -> Result<(), String> {
    match std::env::var_os("CRUNDI_OPEN_URL_FILE") {
        Some(f) => std::fs::write(f, url).map_err(|e| e.to_string()),
        None => open::that(url).map_err(|e| e.to_string()),
    }
}

/// A sign-in call's failure in the web's words.
fn web_error(e: ApiError, fallback: &str) -> String {
    match e {
        ApiError::Net(m) => format!("Could not reach the server: {m}"),
        ApiError::Status(_, m) if !m.is_empty() => m,
        _ => fallback.to_string(),
    }
}

fn st(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }
