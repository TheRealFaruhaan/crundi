//! Saved passwords in the browser drawer (specs/desktop/passwords-homepool-cru.md
//! A; app/browser-drawer.js, app/drawer-passwords.js, the drawer's renderPw
//! and showPasswords): the store (pwstore.rs) with its key in the OS
//! keychain, the page script that reports logins (an isolated world, its
//! lines taken out of the tab's console), the verdict on a submit, the
//! offers (save, update, fill) shown as the bar under the address bar, the
//! lock button's count, filling, the Passwords panel with the PIN, and
//! taking a tab's passwords out of everything a chat reads back.
//!
//! Only the app's own UI reaches these (drawer.rs, ui.rs): no browser tool
//! of a chat calls them.

use crate::{ctx as kit, drawer, pwstore::{origin_of, Store}};
use crundi_webview::{engine as browser, later};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::{HashMap, HashSet}, sync::Mutex, time::{Duration, Instant}};

/// The page script's world (WORLD), and the fill's.
const WORLD: &str = "crundi-pw";
const FILL_WORLD: &str = "crundi-pw-fill";
/// A submit's verdict waits this long (PW_VERDICT_MS); an offer lives this long (PW_OFFER_MS).
const VERDICT: Duration = Duration::from_secs(20);
const OFFER: Duration = Duration::from_secs(5 * 60);
/// The PIN, given once, shows passwords for this long (pwUnlocked).
const UNLOCKED: Duration = Duration::from_secs(5 * 60);

const NO_STORE: &str = "This computer offers no encrypted storage for this app, so passwords cannot be saved.";
const NO_KEYRING: &str = "No desktop keyring (GNOME Keyring or KWallet) is running, so passwords could not be kept safely and are not saved.";

/// The bar under the address bar, for one tab.
#[derive(Clone, Debug, PartialEq)]
pub enum Bar {
    /// Save (or update: `update`) the login just used.
    Save { offer: u64, origin: String, username: String, update: bool },
    /// Fill one of this site's saved logins.
    Fill { origin: String, logins: Vec<(String, String)> },
}

struct Pending { origin: String, username: String, password: String, kind: &'static str, navigated: bool }
struct Offer { tab: String, origin: String, username: String, password: String, at: Instant }

#[derive(Default)]
struct St {
    store: Option<Store>,
    /// Whether passwords can be kept, and why not.
    available: bool,
    reason: String,
    ready: bool,
    prefix: String,
    /// Per tab: every password it was given or typed (redaction), a pending
    /// submit, its bar.
    secrets: HashMap<String, Vec<String>>,
    pending: HashMap<String, Pending>,
    bars: HashMap<String, Bar>,
    offers: HashMap<u64, Offer>,
    next: u64,
    /// Sites whose fill was dismissed (until the app restarts).
    dismissed: HashSet<String>,
    unlocked: Option<Instant>,
    /// The Passwords panel: shown passwords by id, the login waiting for the PIN.
    revealed: HashMap<String, String>,
    pin_for: Option<String>,
}

thread_local! { static S: RefCell<St> = RefCell::new(St::default()); }
fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { S.with(|s| f(&mut s.borrow_mut())) }

/// The key from the keychain (made the first time), off the UI thread.
static KEY: Mutex<Option<Result<[u8; 32], String>>> = Mutex::new(None);

fn file() -> std::path::PathBuf { crundi_core::store::config_dir().join("drawer-passwords.json") }

/// The keychain's key: one random 256-bit key under "Crundi Client" /
/// "drawer-passwords". Under the test driver (CRUNDI_TEST_PORT), a key may
/// be given in CRUNDI_PW_TEST_KEY (64 hex digits) instead, so a headless
/// test without a desktop keyring can save and fill.
fn load_key() -> Result<[u8; 32], String> {
    if std::env::var_os("CRUNDI_TEST_PORT").is_some() {
        if let Ok(h) = std::env::var("CRUNDI_PW_TEST_KEY") {
            let b: Vec<u8> = (0..h.len() / 2).filter_map(|i| u8::from_str_radix(h.get(i * 2..i * 2 + 2)?, 16).ok()).collect();
            if let Ok(k) = <[u8; 32]>::try_from(b.as_slice()) { return Ok(k); }
        }
    }
    keychain()
}

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn keychain() -> Result<[u8; 32], String> {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD;
    let e = keyring::Entry::new("Crundi Client", "drawer-passwords").map_err(|e| reason_of(&e))?;
    match e.get_password() {
        Ok(s) => b64.decode(s.trim()).ok().and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok()).ok_or_else(|| NO_STORE.to_string()),
        Err(keyring::Error::NoEntry) => {
            let mut k = [0u8; 32];
            crate::pwstore::getrandom(&mut k);
            e.set_password(&b64.encode(k)).map_err(|e| reason_of(&e))?;
            Ok(k)
        }
        Err(err) => Err(reason_of(&err)),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn keychain() -> Result<[u8; 32], String> { Err(NO_STORE.into()) }

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn reason_of(e: &keyring::Error) -> String {
    eprintln!("[passwords] keychain: {e}");
    if cfg!(target_os = "linux") { NO_KEYRING.into() } else { NO_STORE.into() }
}

/// At start: a random prefix for the page script's lines, the key in the background.
pub fn init() {
    let mut p = [0u8; 8];
    crate::pwstore::getrandom(&mut p);
    let prefix = format!("\u{2063}crundi-pw:{}:", p.iter().map(|b| format!("{b:02x}")).collect::<String>());
    with(|s| s.prefix = prefix);
    // (Its arrival wakes the pump, which opens the store: no polling.)
    std::thread::spawn(|| { let k = load_key(); *KEY.lock().unwrap() = Some(k); crundi_webview::wake(); });
}

/// The key came (the area's pump): the store opens, or the reason stays.
pub fn pump() {
    if with(|s| s.ready) { return; }
    let Some(k) = KEY.lock().unwrap().take() else { return };
    with(|s| {
        s.ready = true;
        match k {
            Ok(key) => { s.store = Some(Store::new(file(), Some(key))); s.available = true; }
            Err(r) => { s.store = Some(Store::new(file(), None)); s.available = false; s.reason = r; }
        }
    });
    drawer::render();
}

/// Can passwords be kept here, and why not.
pub fn status() -> (bool, String) { with(|s| (s.available, if s.ready { s.reason.clone() } else { String::new() })) }

fn store<R>(f: impl FnOnce(&mut Store) -> R) -> Option<R> { with(|s| s.store.as_mut().map(f)) }

fn host_of(origin: &str) -> String { origin.split("://").nth(1).unwrap_or(origin).to_string() }

// ─── The page script ───

/// The capture script (app/drawer-passwords.js captureScript), word for word.
fn capture_script(prefix: &str) -> String {
    let p = serde_json::to_string(prefix).unwrap_or_default();
    CAPTURE.replace("__PREFIX__", &p)
}

const CAPTURE: &str = r#"(function () {
  if (window.__crundiPwOn) return; window.__crundiPwOn = true;
  var prefix = __PREFIX__, debug = console.debug.bind(console);
  function send(o) { o.origin = location.origin; o.top = window === window.top; try { debug(prefix + JSON.stringify(o)); } catch (e) { /* gone */ } }
  function shown(el) { try { return el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden'; } catch (e) { return false; } }
  function pwFields(root) { return Array.prototype.filter.call(root.querySelectorAll('input[type=password]'), shown); }
  function grab(scope) {
    var root = scope && scope.querySelectorAll ? scope : document;
    var pws = pwFields(root).filter(function (p) { return p.value; });
    if (!pws.length && root !== document) pws = pwFields(document).filter(function (p) { return p.value; });
    if (!pws.length) return null;
    var pw = pws[pws.length - 1];
    var box = pw.form || document;
    var users = Array.prototype.filter.call(box.querySelectorAll('input'), function (i) {
      var t = (i.getAttribute('type') || 'text').toLowerCase();
      return (t === 'text' || t === 'email' || t === 'tel') && i.value && shown(i);
    });
    var user = '';
    users.forEach(function (i) { if (i.compareDocumentPosition(pw) & 4) user = i.value; });
    if (!user && users.length) user = users[0].value;
    return { username: String(user).slice(0, 300), password: String(pw.value).slice(0, 1000) };
  }
  var told = null;
  function look() { var has = pwFields(document).length > 0; if (has !== told) { told = has; send({ t: 'page', hasLogin: has }); } }
  var last = '', watching = null;
  function after() {
    clearInterval(watching); var n = 0;
    watching = setInterval(function () {
      n++; var still = pwFields(document).length > 0;
      if (!still || n >= 16) { clearInterval(watching); watching = null; send({ t: 'after', stillForm: still }); }
    }, 500);
  }
  function maybe(scope) {
    var c = grab(scope); if (!c) return;
    var k = c.username + '\u0000' + c.password; if (k === last) return; last = k;
    c.t = 'submit'; send(c); after();
  }
  document.addEventListener('submit', function (e) { maybe(e.target); }, true);
  document.addEventListener('click', function (e) {
    var b = e.target && e.target.closest && e.target.closest('button, input[type=submit], input[type=button], [role=button]');
    if (b) setTimeout(function () { maybe(b.form || null); }, 0);
  }, true);
  document.addEventListener('keydown', function (e) {
    if (e.key === 'Enter' && e.target && String(e.target.type).toLowerCase() === 'password') maybe(e.target.form || null);
  }, true);
  var moT = null;
  function watchForms() {
    try {
      new MutationObserver(function () { if (moT) return; moT = setTimeout(function () { moT = null; look(); }, 400); })
        .observe(document.documentElement || document, { childList: true, subtree: true, attributes: true, attributeFilter: ['type', 'style', 'class', 'hidden'] });
    } catch (e) { /* no DOM yet */ }
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', function () { look(); watchForms(); });
  else { look(); watchForms(); }
})();"#;

/// The fill script (fillScript), word for word.
fn fill_script(origin: &str, user: &str, pw: &str) -> String {
    let j = |s: &str| serde_json::to_string(s).unwrap_or_default();
    format!(r#"(function (o, u, p) {{
  if (location.origin !== o) return {{ ok: false, error: 'The page is no longer on that site' }};
  function shown(el) {{ try {{ return el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden' && !el.disabled && !el.readOnly; }} catch (e) {{ return false; }} }}
  var pw = Array.prototype.filter.call(document.querySelectorAll('input[type=password]'), shown)[0] || null;
  function set(el, v) {{
    var proto = Object.getPrototypeOf(el);
    var d = Object.getOwnPropertyDescriptor(proto, 'value') || Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
    el.focus();
    d.set.call(el, v);
    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }}
  var box = (pw && pw.form) || document;
  var users = Array.prototype.filter.call(box.querySelectorAll('input'), function (i) {{
    var t = (i.getAttribute('type') || 'text').toLowerCase();
    return (t === 'text' || t === 'email' || t === 'tel') && shown(i);
  }});
  var user = null;
  if (pw) users.forEach(function (i) {{ if (i.compareDocumentPosition(pw) & 4) user = i; }});
  if (!user) user = users.filter(function (i) {{ return /user|email|login|account|name|phone/i.test((i.name || '') + ' ' + (i.id || '') + ' ' + (i.autocomplete || '') + ' ' + (i.placeholder || '')); }})[0] || null;
  if (!pw && !user) return {{ ok: false, error: 'There is no login form on this page' }};
  if (user && u) set(user, u);
  if (pw) set(pw, p);
  return {{ ok: true, user: !!user, password: !!pw }};
}})({}, {}, {})"#, j(origin), j(user), j(pw))
}

/// A drawer tab's page was made: the capture script goes in (CDP
/// Page.enable + addScriptToEvaluateOnNewDocument in its own world; never
/// Runtime.enable: anti-bot scripts see it).
pub fn attach(i: usize) { attach_try(i, 0); }

fn attach_try(i: usize, tries: u32) {
    if browser::with(|st| st.panels.get(i).is_none_or(|p| p.browser.is_none())) {
        if tries < 100 { later(Duration::from_millis(50), move || attach_try(i, tries + 1)); }
        return;
    }
    let src = capture_script(&with(|s| s.prefix.clone()));
    browser::cdp(i, "Page.enable", json!({}), Box::new(|_, _| {}));
    browser::cdp(i, "Page.addScriptToEvaluateOnNewDocument", json!({ "source": src, "worldName": WORLD, "runImmediately": true }), Box::new(|_, _| {}));
}

/// A console line of a page: ours (taken out), or not.
pub fn console(i: usize, text: &str) -> bool {
    let prefix = with(|s| s.prefix.clone());
    if prefix.is_empty() { return false; }
    let Some(rest) = text.strip_prefix(prefix.as_str()) else { return false };
    if let Ok(v) = serde_json::from_str::<Value>(rest) { report(i, &v); }
    true
}

fn report(i: usize, v: &Value) {
    let Some(tab) = drawer::tab_of_panel(i).map(|t| t.id) else { return };
    let url = browser::with(|st| st.panels.get(i).map(|p| p.url.clone()).unwrap_or_default());
    let origin = kit::s(v, "origin");
    // Only the top frame, on the tab's own site.
    if !v.get("top").and_then(|t| t.as_bool()).unwrap_or(false) || origin_of(&url).as_deref() != Some(origin.as_str()) { return; }
    match kit::s(v, "t").as_str() {
        "submit" => {
            let (user, pw) = (kit::s(v, "username"), kit::s(v, "password"));
            if pw.is_empty() { return; }
            with(|s| { let x = s.secrets.entry(tab.clone()).or_default(); if !x.contains(&pw) { x.push(pw.clone()); } });
            let kind = store(|st| st.check(&origin, &user, &pw)).unwrap_or("new");
            if kind == "never" || kind == "same" { return; }
            with(|s| s.pending.insert(tab.clone(), Pending { origin, username: user, password: pw, kind, navigated: false }));
            let t2 = tab.clone();
            // No word in time: a new login worked, a changed one did not.
            later(VERDICT, move || {
                let k = with(|s| s.pending.get(&t2).map(|p| p.kind));
                match k { Some("new") => verdict(&t2, true), Some(_) => verdict(&t2, false), None => {} }
            });
        }
        "after" => if with(|s| s.pending.contains_key(&tab)) { verdict(&tab, !v.get("stillForm").and_then(|x| x.as_bool()).unwrap_or(true)); },
        "page" => {
            let has = v.get("hasLogin").and_then(|x| x.as_bool()).unwrap_or(false);
            let pending = with(|s| s.pending.get(&tab).map(|p| p.navigated));
            match pending {
                Some(_) if !has => verdict(&tab, true),
                Some(true) if has => verdict(&tab, false),
                Some(_) => {}
                None => if has { offer_fill(&tab, &origin); },
            }
        }
        _ => {}
    }
}

fn verdict(tab: &str, worked: bool) {
    let Some(p) = with(|s| s.pending.remove(tab)) else { return };
    if !worked { eprintln!("[passwords] login on {} looks refused: nothing offered", p.origin); return; }
    let (available, reason) = status();
    if !available { return kit::toast_error(&format!("A login was not saved: {reason}")); }
    let id = with(|s| {
        s.next += 1;
        let id = s.next;
        s.offers.retain(|_, o| o.at.elapsed() < OFFER);
        s.offers.insert(id, Offer { tab: tab.to_string(), origin: p.origin.clone(), username: p.username.clone(), password: p.password.clone(), at: Instant::now() });
        // An offer replaces any fill bar.
        s.bars.insert(tab.to_string(), Bar::Save { offer: id, origin: p.origin.clone(), username: p.username.clone(), update: p.kind == "update" });
        id
    });
    let _ = id;
    drawer::render();
}

fn offer_fill(tab: &str, origin: &str) {
    if with(|s| s.dismissed.contains(origin) || matches!(s.bars.get(tab), Some(Bar::Save { .. }))) { return; }
    let logins: Vec<(String, String)> = store(|st| st.list(Some(origin))).unwrap_or_default().into_iter().map(|l| (l.id, l.username)).collect();
    if logins.is_empty() { return; }
    with(|s| s.bars.insert(tab.to_string(), Bar::Fill { origin: origin.to_string(), logins }));
    drawer::render();
}

/// A tab's address changed: a pending submit has navigated; a bar for
/// another site goes.
pub fn visited(tab: &str, url: &str) {
    let o = origin_of(url).unwrap_or_default();
    with(|s| {
        if let Some(p) = s.pending.get_mut(tab) { p.navigated = true; }
        let gone = match s.bars.get(tab) { Some(Bar::Save { origin, .. }) | Some(Bar::Fill { origin, .. }) => *origin != o, None => false };
        if gone { s.bars.remove(tab); }
    });
}

/// The active tab's bar.
pub fn bar(tab: &str) -> Option<Bar> { with(|s| s.bars.get(tab).cloned()) }

/// How many logins are saved for a page's site (the lock button).
pub fn count_for(url: &str) -> usize { origin_of(url).and_then(|o| store(|st| st.count(&o))).unwrap_or(0) }

// ─── The bar's answers ───

/// Save (or update) the offered login, with the username as typed. The PIN
/// must be set first (it protects the passwords).
pub fn save(tab: &str, username: &str) {
    let Some(Bar::Save { offer, update, .. }) = bar(tab) else { return };
    let username = username.to_string();
    let tab = tab.to_string();
    kit::call(|api| api.get("/api/lock"), move |r| {
        if !matches!(&r, Ok(v) if kit::truthy(&v["pinSet"])) {
            return kit::toast_error("Set your PIN first (Settings, Voice assistant, Your PIN): it protects your saved passwords.");
        }
        let Some(o) = with(|s| s.offers.remove(&offer)).filter(|o| o.at.elapsed() < OFFER) else { with(|s| s.bars.remove(&tab)); drawer::render(); return kit::toast_error("That offer has expired") };
        let user = if update { o.username.clone() } else { username.clone() };
        match store(|st| st.save(&o.origin, &user, &o.password)) {
            Some(Ok(_)) => kit::toast_ok(if update { "Password updated" } else { "Password saved" }),
            Some(Err(e)) => kit::toast_error(&format!("Not saved: {e}")),
            None => kit::toast_error("Not saved"),
        }
        with(|s| s.bars.remove(&o.tab));
        drawer::render();
    });
}

pub fn never(tab: &str) {
    let Some(Bar::Save { offer, origin, .. }) = bar(tab) else { return };
    store(|st| st.never(&origin));
    with(|s| { s.offers.remove(&offer); s.bars.remove(tab); });
    kit::toast(&format!("Passwords for {} will not be offered again (Passwords, in the menu, undoes it)", host_of(&origin)));
    drawer::render();
}

/// × on the bar: not now (a fill bar: not for this site until the app restarts).
pub fn dismiss(tab: &str) {
    let b = with(|s| s.bars.remove(tab));
    match b {
        Some(Bar::Fill { origin, .. }) => with(|s| { s.dismissed.insert(origin); }),
        Some(Bar::Save { offer, .. }) => with(|s| { s.offers.remove(&offer); }),
        None => {}
    }
    drawer::render();
}

/// Fill a saved login into the tab's page (an isolated world of its own).
pub fn fill(tab: &str, id: &str) {
    let Some((_, t)) = drawer::find(tab) else { return kit::toast_error("No such tab") };
    let Some(i) = t.panel else { return kit::toast_error("Could not fill this page") };
    let (origin, user, pw) = match store(|st| st.password(id)) { Some(Ok(x)) => x, Some(Err(e)) => return kit::toast_error(&e), None => return };
    let url = browser::with(|st| st.panels.get(i).map(|p| p.url.clone()).unwrap_or_default());
    if origin_of(&url).as_deref() != Some(origin.as_str()) { return kit::toast_error(&format!("This tab is not on {}", host_of(&origin))); }
    with(|s| { let x = s.secrets.entry(tab.to_string()).or_default(); if !x.contains(&pw) { x.push(pw.clone()); } s.bars.remove(tab); });
    drawer::render();
    let code = fill_script(&origin, &user, &pw);
    let id = id.to_string();
    browser::cdp(i, "Page.getFrameTree", json!({}), Box::new(move |ok, v| {
        let frame = v.pointer("/frameTree/frame/id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if !ok || frame.is_empty() { return kit::toast_error("Could not fill this page"); }
        browser::cdp(i, "Page.createIsolatedWorld", json!({ "frameId": frame, "worldName": FILL_WORLD }), Box::new(move |ok, v| {
            let ctxid = v.get("executionContextId").cloned().unwrap_or(Value::Null);
            if !ok || ctxid.is_null() { return kit::toast_error("Could not fill this page"); }
            browser::cdp(i, "Runtime.evaluate", json!({ "expression": code, "contextId": ctxid, "returnByValue": true }), Box::new(move |ok, v| {
                let r = v.pointer("/result/value").cloned().unwrap_or(Value::Null);
                if ok && r.get("ok").and_then(|x| x.as_bool()) == Some(true) { store(|st| st.used(&id)); }
                else { kit::toast_error(r.get("error").and_then(|e| e.as_str()).unwrap_or("Could not fill this page")); }
            }));
        }));
    }));
}

/// The lock button (pwSiteMenu): none, a toast; one, filled; several, the fill bar.
pub fn site_menu() {
    let Some(t) = drawer::active_tab() else { return };
    let Some(origin) = origin_of(&t.url) else { return kit::toast("No saved login for this site") };
    let logins: Vec<(String, String)> = store(|st| st.list(Some(&origin))).unwrap_or_default().into_iter().map(|l| (l.id, l.username)).collect();
    match logins.len() {
        0 => kit::toast("No saved login for this site"),
        1 => fill(&t.id, &logins[0].0),
        _ => { with(|s| s.bars.insert(t.id.clone(), Bar::Fill { origin, logins })); drawer::render(); }
    }
}

// ─── The Passwords panel ───

/// Every saved login: (id, username, host), and the sites never offered.
pub fn panel_rows() -> (Vec<(String, String, String)>, Vec<String>) {
    let rows = store(|st| st.list(None)).unwrap_or_default().into_iter().map(|l| (l.id, l.username, host_of(&l.origin))).collect();
    let never = store(|st| st.never_list()).unwrap_or_default();
    (rows, never)
}

/// The login waiting for the PIN.
pub fn pin_for() -> Option<String> { with(|s| s.pin_for.clone()) }
/// Every shown password (the panel's copy), by id.
pub fn revealed_all() -> Vec<(String, String)> { let mut v: Vec<_> = with(|s| s.revealed.iter().map(|(k, x)| (k.clone(), x.clone())).collect()); v.sort(); v }

/// Show (or hide) a password: at once while the PIN was given lately, else the PIN row.
pub fn show(id: &str) {
    if with(|s| s.revealed.remove(id).is_some()) { return drawer::render(); }
    if with(|s| s.unlocked.is_some_and(|t| t.elapsed() < UNLOCKED)) { return reveal(id); }
    with(|s| s.pin_for = Some(id.to_string()));
    drawer::render();
}

fn reveal(id: &str) {
    match store(|st| st.password(id)) {
        Some(Ok((_, _, pw))) => { with(|s| { s.revealed.insert(id.to_string(), pw); s.pin_for = None; }); }
        _ => kit::toast_error("Could not show it"),
    }
    drawer::render();
}

/// Unlock with the PIN (POST /api/lock/check; no side effects on the server).
pub fn unlock(pin: &str) {
    let Some(id) = pin_for() else { return };
    let body = json!({ "pin": pin });
    kit::call(move |api| api.post("/api/lock/check", &body), move |r| match r {
        Ok(v) if kit::truthy(&v["ok"]) => {
            with(|s| s.unlocked = Some(Instant::now()));
            // A wrong PIN's toast goes once the right one is in (F81).
            kit::on_app(|win, cx| crundi_widgets::clear_toasts(win, cx));
            reveal(&id);
        }
        Ok(v) => kit::toast_error(&Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "That is not the PIN".into())),
        Err(e) => kit::toast_error(if e.is_empty() { "That is not the PIN" } else { &e }),
    });
}

pub fn remove(id: &str) {
    let id = id.to_string();
    kit::confirm("Delete this saved password?", "Delete", true, move |ok| {
        if ok { store(|st| st.remove(&id)); with(|s| { s.revealed.remove(&id); }); drawer::render(); }
    });
}

pub fn unnever(origin: &str) { store(|st| st.unnever(origin)); drawer::render(); }

/// The panel opened (nothing to do: drawn from the store) or closed (shown passwords hidden again).
pub fn show_panel() {}
pub fn panel_closed() { with(|s| { s.revealed.clear(); s.pin_for = None; }); }

/// Clear browsing data's "Saved passwords".
pub fn clear_all() { store(|st| st.clear()); drawer::render(); }

// ─── Redaction ───

/// Every password a drawer tab was given or typed, out of what a chat reads
/// back from it (redactSecrets): replaced by ••••••••; binary fields
/// (data, image, png, pdf) are left alone.
pub fn redact(key: &str, v: Value) -> Value {
    let Some(tab) = key.strip_prefix("drawer:") else { return v };
    let mut list: Vec<String> = with(|s| s.secrets.get(tab).cloned().unwrap_or_default()).into_iter().filter(|x| x.chars().count() >= 4).collect();
    if list.is_empty() { return v; }
    list.sort_by_key(|b| std::cmp::Reverse(b.len()));
    redact_with(v, &list, "")
}

pub fn redact_with(v: Value, list: &[String], key: &str) -> Value {
    match v {
        Value::String(s) if !matches!(key, "data" | "image" | "png" | "pdf") => {
            let mut out = s;
            for x in list { if out.contains(x.as_str()) { out = out.replace(x.as_str(), "••••••••"); } }
            Value::String(out)
        }
        Value::Array(a) => Value::Array(a.into_iter().map(|x| redact_with(x, list, "")).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, x)| { let y = redact_with(x, list, &k); (k, y) }).collect()),
        other => other,
    }
}

/// What the test driver sees.
pub fn props() -> Value {
    let (available, reason) = status();
    let (rows, never) = panel_rows();
    with(|s| json!({
        "available": available, "reason": reason, "ready": s.ready,
        "logins": rows.iter().map(|(id, u, h)| json!({ "id": id, "username": u, "host": h })).collect::<Vec<_>>(), "never": never,
        "bars": s.bars.iter().map(|(t, b)| (t.clone(), match b {
            Bar::Save { origin, username, update, .. } => json!({ "kind": if *update { "update" } else { "save" }, "origin": origin, "username": username }),
            Bar::Fill { origin, logins } => json!({ "kind": "fill", "origin": origin, "logins": logins.iter().map(|l| json!({ "id": l.0, "username": l.1 })).collect::<Vec<_>>() }),
        })).collect::<serde_json::Map<_, _>>(),
        "pending": s.pending.keys().cloned().collect::<Vec<_>>(),
        "revealed": s.revealed.keys().cloned().collect::<Vec<_>>(), "pinFor": s.pin_for,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_but_not_binaries() {
        let v = json!({ "result": "my pw is hunter22", "image": "hunter22", "list": ["hunter22 x"], "n": 3 });
        let r = redact_with(v, &["hunter22".to_string()], "");
        assert_eq!(r["result"], "my pw is ••••••••");
        assert_eq!(r["image"], "hunter22");
        assert_eq!(r["list"][0], "•••••••• x");
        assert_eq!(r["n"], 3);
    }

    #[test]
    fn scripts() {
        assert!(capture_script("\u{2063}crundi-pw:ab:").contains(r#"var prefix = "⁣crundi-pw:ab:""#) || capture_script("x").contains("var prefix = \"x\""));
        let f = fill_script("https://a.b", "me", "p'\"w");
        assert!(f.ends_with(r#"})("https://a.b", "me", "p'\"w")"#));
    }
}
