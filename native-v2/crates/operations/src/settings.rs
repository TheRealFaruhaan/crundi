//! Settings (the Settings tab, all of it for the owner; Slint:
//! src/ops/settings.rs + settings/, ui/ops/settings*.slint,
//! specs/ops/settings.md): the web's renderSettings and organizeSettings,
//! the update sections, the phone app, the session, Cru's server settings,
//! and the PC client's own "Desktop App" section. Skills, Usage and Backup &
//! restore are skills.rs's, placed here in their groups.
//!
//! The state is one Global (`St`): the server's answers, which sections are
//! open, the text boxes' text (`Fields`). The page is a view over it
//! (`Page`), redrawn when the state changes. Pure rules: settings/rules.rs.

mod local;
pub mod rules;

use crate::skills::common::{self as c, Fields, HasFields};
use crate::compat::AreaEvent;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use crate::rows::{parts, slice, Parts, Rows, Want};
use rules::*;
use serde_json::{json, Map, Value};
use std::{cell::{Cell, RefCell}, collections::HashMap, rc::Rc, sync::Arc, time::{Duration, Instant}};

// ─── State ───

/// The sections, by key, in the page's order.
pub const SECTIONS: &[&str] = &["server", "cc", "whatsnew", "sysprompt", "skills", "usage", "voice", "channels", "notify", "signin", "phone", "session", "config", "terminal", "backup", "desktop"];

/// The text boxes the driver can type into (its names), and the screen's own.
const FIELDS: &[&str] = &["projects-dir", "web-port", "tunnel-token", "tunnel-url", "data-dir", "sysprompt", "proj-sysprompt",
    "new-password", "tg-token", "tg-user", "chat-id", "google-id", "google-secret", "google-allowed", "github-id", "github-secret", "github-allowed",
    "push-key", "relay-url", "autopark", "vs-limit", "vs-bday", "vs-dlimit", "vs-dday", "vs-akey", "vs-dkey", "vs-xkey"];

#[derive(Default, Clone)]
struct Chan { id: String, label: String, hint: String, enabled: bool, can_toggle: bool, available: bool, telegram: bool }

#[derive(Default, Clone)]
struct OAuth { v: OAuthView, test_line: String }

struct OAuthPoll { epoch: u64, poll_id: String, until: Instant, busy: bool }

#[derive(Default)]
struct Pair {
    code: String, link: String, qr: String, mark: String, showing_qr: bool, note: String, warn: String,
    left: i64, paired: bool, epoch: u64,
}

/// Cru's server settings (showVoiceServer): each select's labels, the chosen
/// one and the values to send; the rest as the Slint client's properties.
#[derive(Default)]
struct Voice {
    shown: bool, has_brain: bool, has_effort: bool, effort_hint: String, has_volume: bool, has_sclose: bool,
    sel: HashMap<&'static str, (Vec<String>, i32, Vec<Value>)>,
    has_admin: bool, admin: bool, admin_hint: String,
    has_away: bool, away: bool, away_classes: Vec<(String, String, bool)>, away_note: String, away_bad: bool,
    spend: String, akey_set: bool, dkey_set: bool, xkey_set: bool,
    status: String, status_bad: bool, spend_note: String, spend_bad: bool, keys_note: String, keys_bad: bool, reset_note: String, reset_bad: bool,
}

pub struct St {
    shown: bool,
    loaded: bool,
    state: String,
    error: String,
    open: HashMap<&'static str, bool>,
    data: Value,
    prefs: Map<String, Value>,
    sp_max: usize,
    autopark: i64,
    autopark_min: i64,
    // Server and Claude Code.
    srv_body: String, srv_error: String, srv: ServerView, srv_checked: Option<Instant>, srv_poll: u64, srv_polling: bool,
    cc_body: String, cc_error: String, cc: ClaudeView,
    // Configuration, prompts, terminal.
    env_path: String, config_status: String, config_tone: String,
    proj_name: String, sp_status: String, sp_tone: String,
    proj_prompts: HashMap<String, String>,
    newline_key: i32,
    // Notifications.
    notify: Vec<NotifyRow>, channels: Vec<Chan>, tgc_tone: String, tgc_state: String, tgc_token_set: bool,
    // Sign-in.
    pw_set: bool, pw_on: bool, enrol: String,
    tg_has: bool, tg_on: bool, tg_can: bool, tg_status: String, tg_token_set: bool, tg_restart: bool,
    oauth_has: bool, google: OAuth, github: OAuth, oauth_polls: HashMap<String, OAuthPoll>, epoch: u64,
    // Phone app.
    push_state: String, push_line: String, push_contact: String, push_contact_url: String, push_error: String, push_has_key: bool,
    relay_summary: String, relay_custom: bool, relay_open: bool,
    pair: Option<Pair>, pair_open: bool, pair_again: bool, pair_line: String, pair_done: bool, pair_small: String,
    pair_pic_text: String, pair_swap: String, pair_copy: String, pair_image: Option<Arc<Image>>,
    pair_focus: Option<FocusHandle>, pair_scroll: ScrollHandle,
    // Session.
    sess_open: bool, sess_state: String, sess_error: String, sessions: Vec<SessionRow>, sess_others: i32,
    // Cru.
    voice: Voice,
    // The Desktop App.
    launch: bool, launch_hint: String, auto_update: bool, upd_channel: i32, upd_avail: bool, upd_status: String,
    /// The updater's download: under way, finished (the action button's words).
    upd_downloading: bool, upd_downloaded: bool,
    f: Fields,
    page: Option<Entity<Page>>,
    /// The signed-in server's address (Session), kept here for drawing.
    server: String,
    /// Waiting for the new push-to-talk shortcut (the page takes the keys).
    ptt_capturing: bool,
    /// The line beside Reset (#voice-ptt-status): what to do while
    /// capturing, "Saved on this device" once set, nothing after Esc.
    ptt_status: &'static str,
    ptt_focus: Option<FocusHandle>,
    scroll_to: Option<&'static str>,
    /// The driver asked to bring the open skill into view (its row's place as last laid out).
    reveal_skill: bool,
}

/// The handle to the state (a Global holds only it: ARCHITECTURE.md rule 1).
struct StH(Entity<St>);
impl Global for StH {}
impl HasFields for St {
    fn handle(cx: &App) -> Option<Entity<St>> { cx.try_global::<StH>().map(|h| h.0.clone()) }
    fn fields(&mut self) -> &mut Fields { &mut self.f }
}

impl Default for St {
    fn default() -> Self {
        let l = local::load();
        St {
            shown: false, loaded: false, state: "loading".into(), error: String::new(),
            open: SECTIONS.iter().map(|k| (*k, false)).collect(),
            data: Value::Null, prefs: Map::new(), sp_max: 8000, autopark: 60, autopark_min: 10,
            srv_body: "checking".into(), srv_error: String::new(), srv: ServerView::default(), srv_checked: None, srv_poll: 0, srv_polling: false,
            cc_body: "checking".into(), cc_error: String::new(), cc: ClaudeView::default(),
            env_path: String::new(), config_status: String::new(), config_tone: String::new(),
            proj_name: String::new(), sp_status: String::new(), sp_tone: String::new(), proj_prompts: HashMap::new(),
            newline_key: if l.newline_key() == "ctrl" { 1 } else { 0 },
            notify: vec![], channels: vec![], tgc_tone: String::new(), tgc_state: String::new(), tgc_token_set: false,
            pw_set: false, pw_on: false, enrol: String::new(),
            tg_has: false, tg_on: false, tg_can: false, tg_status: String::new(), tg_token_set: false, tg_restart: false,
            oauth_has: false, google: OAuth::default(), github: OAuth::default(), oauth_polls: HashMap::new(), epoch: 0,
            push_state: "checking".into(), push_line: String::new(), push_contact: String::new(), push_contact_url: String::new(), push_error: String::new(), push_has_key: false,
            relay_summary: String::new(), relay_custom: false, relay_open: false,
            pair: None, pair_open: false, pair_again: false, pair_line: String::new(), pair_done: false, pair_small: String::new(),
            pair_pic_text: String::new(), pair_swap: String::new(), pair_copy: String::new(), pair_image: None,
            pair_focus: None, pair_scroll: ScrollHandle::new(),
            sess_open: false, sess_state: "loading".into(), sess_error: String::new(), sessions: vec![], sess_others: 0,
            voice: Voice::default(),
            launch: local::launch(), launch_hint: local::launch_hint(), auto_update: local::auto_update(),
            upd_channel: if local::channel() == "dev" { 1 } else { 0 }, upd_avail: false,
            upd_status: format!("Up to date \u{2014} v{}", crundi_shell::lifecycle::version()),
            upd_downloading: false, upd_downloaded: false,
            f: Fields::default(),
            page: None,
            server: String::new(),
            ptt_capturing: false,
            ptt_status: "",
            ptt_focus: None,
            scroll_to: None,
            reveal_skill: false,
        }
    }
}

fn ent(cx: &mut App) -> Entity<St> {
    if let Some(h) = cx.try_global::<StH>() { return h.0.clone(); }
    let e = cx.new(|_| St::default());
    cx.set_global(StH(e.clone()));
    e
}

fn ensure(cx: &mut App) { ent(cx); }

/// Change the state; the page (which observes it) draws again.
fn upd<R>(cx: &mut App, f: impl FnOnce(&mut St) -> R) -> R {
    let e = ent(cx);
    e.update(cx, |s, cx| { let r = f(s); cx.notify(); r })
}

/// Change what the page does not show (no redraw).
fn quiet<R>(cx: &mut App, f: impl FnOnce(&mut St) -> R) -> R { let e = ent(cx); e.update(cx, |s, _| f(s)) }

/// The Settings tab's dot in the shell's project bar while an update is available.
pub(crate) fn register_badge(cx: &mut App) {
    let e = ent(cx);
    crundi_shell::register_tab_badge(cx, "settings", &e, |_, cx| get(cx, |s| s.upd_avail).then(|| w::tip(div().id("update-badge").child(w::update_dot()), "Update available").into_any_element()));
}

fn get<R>(cx: &mut App, f: impl FnOnce(&St) -> R) -> R { let e = ent(cx); f(e.read(cx)) }

/// The state, while the page draws (after `ensure`).
/// Another area's part (compat::settings_part_changed) changed: redraw, if the page exists.
pub(crate) fn parts_changed(cx: &mut App) {
    if let Some(e) = cx.try_global::<StH>().map(|h| h.0.clone()) { e.update(cx, |_, cx| cx.notify()); }
}

fn st(cx: &App) -> &St { cx.global::<StH>().0.read(cx) }

fn field(cx: &mut App, k: &str) -> String { get(cx, |s| s.f.get(k)) }
fn set_field(cx: &mut App, k: &'static str, v: impl Into<String>) { let v = v.into(); upd(cx, |s| s.f.set(k, v)); }

fn post(cx: &mut App, path: &'static str, body: Value, done: impl FnOnce(Result<Value, String>, &mut App) + 'static) {
    crundi_shell::call(cx, move |api| api.post(path, &body), done);
}

/// Open or close a section (its fold).
pub fn set_open(cx: &mut App, key: &str, on: bool) -> bool {
    let Some(k) = SECTIONS.iter().find(|x| **x == key).copied() else { return false };
    upd(cx, |s| { s.open.insert(k, on); if on { s.scroll_to = Some(k); } });
    true
}

pub fn is_open(cx: &mut App, key: &str) -> bool { get(cx, |s| s.open.get(key).copied().unwrap_or(false)) }

/// The PC client's own update (the desktop area's updater, Electron's
/// update:status): the Desktop App section's status row (updStatusRowHtml)
/// and the Settings tab's dot.
pub fn set_update_state(cx: &mut App, u: &Value) {
    let b = |k: &str| u.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let st = |k: &str| u.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let (avail, dl, ready, enabled) = (b("available"), b("downloading"), b("downloaded"), b("enabled"));
    let status = if dl {
        format!("Downloading\u{2026} {}%", u.get("percent").and_then(|v| v.as_u64()).unwrap_or(0))
    } else if avail {
        let v = st("version");
        let head = if st("channel") == "dev" { if st("latestKind") == "production" { format!("Production release available \u{2014} v{v}") } else { format!("Dev build available \u{2014} v{v}") } } else { format!("Update available \u{2014} v{v}") };
        if ready { format!("{head} (ready)") } else { head }
    } else {
        let cur = st("current");
        if cur.is_empty() { "Up to date".to_string() } else { format!("Up to date \u{2014} v{cur}") }
    };
    let chan = if st("channel") == "dev" { 1 } else { 0 };
    upd(cx, |s| { s.upd_avail = avail; s.upd_downloading = dl; s.upd_downloaded = ready; s.upd_status = status; s.auto_update = enabled; s.upd_channel = chan; });
}

/// The terminal's newline key ("shift" or "ctrl"), for the terminal panels.
pub fn newline_key() -> String { local::load().newline_key() }

// ─── The registry ───

/// This screen's tab: the page.
pub fn view(tab: &str, _window: &mut Window, cx: &mut App) -> Option<AnyView> {
    if tab != "settings" { return None; }
    ensure(cx);
    let page = match get(cx, |s| s.page.clone()) {
        Some(p) => p,
        None => {
            let (st_e, sk_e) = (ent(cx), crate::skills::ent(cx));
            let p = cx.new(|cx| Page::new(&st_e, &sk_e, cx));
            quiet(cx, |s| s.page = Some(p.clone()));
            p
        }
    };
    Some(page.into())
}

pub fn on_event(e: &AreaEvent, cx: &mut App) {
    // Back online after a restart: what the page says may be stale.
    if let AreaEvent::Connected(true) = e {
        if get(cx, |s| s.shown && s.loaded) { server_update(cx, false); }
    }
}

pub fn on_project(_alias: &str, cx: &mut App) {
    if get(cx, |s| s.loaded) { show_project(cx); }
}

pub fn on_signed_in(_cx: &mut App) {}

pub fn on_signed_out(cx: &mut App) {
    upd(cx, |s| {
        s.loaded = false;
        s.data = Value::Null;
        s.proj_prompts.clear();
        s.srv_poll += 1;
        s.srv_polling = false;
        s.oauth_polls.clear();
        s.epoch += 1;
        s.pair = None;
        s.state = "loading".into();
        s.pair_open = false;
        s.enrol.clear();
        s.sess_open = false;
    });
}

pub fn on_tab(tab: &str, cx: &mut App) {
    let on = tab == "settings";
    upd(cx, |s| s.shown = on);
    if on { render(cx); }
}

// ─── Loading ───

/// The web's renderSettings: the settings, how you sign in and where
/// notifications go; then the sections that load on their own.
fn render(cx: &mut App) {
    if crundi_shell::api(cx).is_none() { return; }
    let server = crate::compat::session::get(cx).server;
    if get(cx, |s| s.server != server) { upd(cx, |s| s.server = server); }
    if !get(cx, |s| s.loaded) { upd(cx, |s| s.state = "loading".into()); }
    crundi_shell::call(cx, |api| {
        let s = api.get("/api/settings")?;
        let a = api.get("/api/auth/config").unwrap_or(Value::Null);
        let c = api.get("/api/notify/channels").unwrap_or(Value::Null);
        Ok(json!({ "s": s, "a": a, "c": c }))
    }, |r, cx| {
        let r = r.and_then(|all| c::ok_or(Ok(all["s"].clone()), "Failed").map(|_| all));
        match r {
            Ok(all) => render_done(cx, all),
            Err(e) => upd(cx, |s| { s.error = e; s.state = "error".into(); }),
        }
    });
}

fn render_done(cx: &mut App, all: Value) {
    let data = all["s"].clone();
    let mut auth = all["a"].clone();
    if auth.get("ok") != Some(&Value::Bool(true)) {
        auth = json!({ "telegram": false, "password": false, "telegramAvailable": false, "passwordSet": false });
    }
    let chans = if all["c"].get("ok") == Some(&Value::Bool(true)) { all["c"]["channels"].as_array().cloned().unwrap_or_default() } else { vec![] };
    let prefs = notify_prefs(data.get("notifyPrefs"));
    let sp_max = data.get("systemPromptMax").and_then(|n| n.as_u64()).unwrap_or(8000) as usize;
    let autopark = data.get("autoParkMinutes").and_then(|n| n.as_i64()).unwrap_or(60);
    let autopark_min = data.get("autoParkMin").and_then(|n| n.as_i64()).filter(|n| *n > 0).unwrap_or(10);
    let set = data.get("settings").cloned().unwrap_or(Value::Null);
    let sess_open = upd(cx, |s| {
        s.loaded = true;
        s.data = json!({ "settings": data.clone(), "auth": auth.clone() });
        s.notify = notify_rows(&prefs);
        s.prefs = prefs;
        s.sp_max = sp_max;
        s.autopark = autopark;
        s.autopark_min = autopark_min;
        s.env_path = c::s(&data, "envPath");
        for (k, n) in [("projects-dir", "PROJECTS_DIR"), ("web-port", "WEB_PORT"), ("tunnel-token", "CLOUDFLARE_TUNNEL_TOKEN"), ("tunnel-url", "CLOUDFLARE_TUNNEL_URL"), ("data-dir", "DATA_DIR")] {
            s.f.set(k, c::s(&set, n));
        }
        s.config_status.clear();
        s.config_tone.clear();
        s.f.set("sysprompt", c::s(&data, "systemPrompt"));
        s.sp_status.clear();
        s.sp_tone.clear();
        s.f.set("autopark", autopark.to_string());
        s.newline_key = if local::load().newline_key() == "ctrl" { 1 } else { 0 };
        show_channels(s, &chans);
        show_auth(s, &auth);
        s.state = "data".into();
        s.sess_open
    });
    show_project(cx);
    // The sections that load by themselves.
    server_update(cx, false);
    claude_update(cx, false);
    push_load(cx);
    voice_load(cx);
    if sess_open { sessions_load(cx); }
    desktop_show(cx);
}

fn show_project(cx: &mut App) {
    let alias = crundi_shell::project(cx);
    let p = crate::compat::session::get(cx).projects.iter().find(|p| c::s(p, "alias") == alias && !alias.is_empty()).cloned();
    upd(cx, |s| {
        let over = s.proj_prompts.get(&alias).cloned();
        match &p {
            Some(p) => {
                let name = c::s(p, "name");
                s.proj_name = if name.is_empty() { alias.clone() } else { name };
                s.f.set("proj-sysprompt", over.unwrap_or_else(|| c::s(p, "systemPrompt")));
            }
            None => { s.proj_name.clear(); s.f.set("proj-sysprompt", ""); }
        }
    });
}

fn show_channels(s: &mut St, chans: &[Value]) {
    let mut rows = Vec::new();
    for ch in chans {
        let available = c::t(ch, "available");
        let always = c::t(ch, "alwaysOn");
        let hint = if available { c::s(ch, "describe") } else {
            let u = c::s(ch, "unavailableReason");
            if u.is_empty() { c::s(ch, "describe") } else { u }
        };
        let id = c::s(ch, "id");
        let tg = id == "telegram" && ch.get("telegram").map(|t| t.is_object()).unwrap_or(false);
        if tg {
            let t = &ch["telegram"];
            let (tone, words) = telegram_channel_state(t);
            s.tgc_tone = tone.into();
            s.tgc_state = words;
            s.tgc_token_set = c::t(t, "tokenSet");
            s.f.set("chat-id", c::s(t, "chatId"));
        }
        rows.push(Chan { id, label: c::s(ch, "label"), hint, enabled: c::t(ch, "enabled"), can_toggle: !always && available, available, telegram: tg });
    }
    s.channels = rows;
}

fn show_auth(s: &mut St, auth: &Value) {
    s.pw_set = c::t(auth, "passwordSet");
    s.pw_on = c::t(auth, "password");
    s.f.set("new-password", "");
    s.tg_on = c::t(auth, "telegram");
    s.tg_can = c::t(auth, "telegramAvailable");
    match auth.get("telegramBot").filter(|b| b.is_object()) {
        Some(tb) => {
            s.tg_has = true;
            s.tg_status = telegram_status(tb);
            s.tg_token_set = c::t(tb, "tokenSet");
            s.tg_restart = c::t(tb, "restartRequired");
            s.f.set("tg-token", "");
            s.f.set("tg-user", c::s(tb, "allowedUsername"));
        }
        None => s.tg_has = false,
    }
    s.oauth_has = auth.get("oauth").map(|o| o.is_object()).unwrap_or(false);
    if s.oauth_has {
        for k in ["google", "github"] {
            let v = oauth_view(auth, k);
            let (id, sec, al): (&'static str, &'static str, &'static str) = if k == "google" { ("google-id", "google-secret", "google-allowed") } else { ("github-id", "github-secret", "github-allowed") };
            s.f.set(id, v.client_id.clone());
            s.f.set(sec, "");
            s.f.set(al, v.allowed.clone());
            let o = if k == "google" { &mut s.google } else { &mut s.github };
            o.v = v;
        }
    }
}

// ─── Configuration, prompts, terminal ───

fn save_config(cx: &mut App) {
    let settings = get(cx, |s| json!({
        "PROJECTS_DIR": s.f.get("projects-dir"), "WEB_PORT": s.f.get("web-port"),
        "CLOUDFLARE_TUNNEL_TOKEN": s.f.get("tunnel-token"), "CLOUDFLARE_TUNNEL_URL": s.f.get("tunnel-url"), "DATA_DIR": s.f.get("data-dir"),
    }));
    upd(cx, |s| { s.config_status = "Saving...".into(); s.config_tone.clear(); });
    post(cx, "/api/settings", json!({ "settings": settings }), |r, cx| match c::ok_or(r, "unknown") {
        Ok(_) => {
            c::toast(cx, "Settings saved. Restart required for changes to take effect.");
            upd(cx, |s| { s.config_status = "Saved \u{2014} restart required".into(); s.config_tone = "warn".into(); });
        }
        Err(e) => {
            c::toast_error(cx, &e);
            upd(cx, |s| { s.config_status = format!("Error: {e}"); s.config_tone.clear(); });
        }
    });
}

fn clip(s: &str, max: usize) -> String { s.chars().take(max).collect() }

fn save_prompts(cx: &mut App) {
    let alias = crundi_shell::project(cx);
    let (all, proj) = get(cx, |s| {
        let all = clip(&s.f.get("sysprompt"), s.sp_max);
        let proj = if s.proj_name.is_empty() || alias.is_empty() { None } else { Some(clip(&s.f.get("proj-sysprompt"), s.sp_max)) };
        (all, proj)
    });
    upd(cx, |s| { s.sp_status = "Saving...".into(); s.sp_tone.clear(); });
    crundi_shell::call(cx, move |api| {
        let r = api.post("/api/settings", &json!({ "systemPrompt": all }))?;
        if r.get("ok") == Some(&Value::Bool(false)) { return Ok(json!({ "ok": false, "error": c::s(&r, "error"), "which": "all" })); }
        if let Some(text) = proj {
            let p = api.post("/api/projects/system-prompt", &json!({ "project": alias, "text": text }))?;
            if p.get("ok") == Some(&Value::Bool(false)) { return Ok(json!({ "ok": false, "error": c::s(&p, "error"), "which": "project" })); }
            return Ok(json!({ "ok": true, "project": alias, "text": text }));
        }
        Ok(json!({ "ok": true }))
    }, |r, cx| {
        let err = |e: String, cx: &mut App| { c::toast_error(cx, &e); upd(cx, |s| { s.sp_status = "Error".into(); s.sp_tone = "error".into(); }); };
        match r {
            Ok(v) if v.get("ok") == Some(&Value::Bool(false)) => {
                let e = c::s(&v, "error");
                err(if !e.is_empty() { e } else if c::s(&v, "which") == "project" { "Could not save the project prompt".into() } else { "Failed to save".into() }, cx);
            }
            Ok(v) => {
                // Keep the shown text in step until the projects list catches up.
                if let Some(p) = v.get("project").and_then(|p| p.as_str()) { let t = c::s(&v, "text"); let p = p.to_string(); upd(cx, |s| s.proj_prompts.insert(p, t)); }
                c::toast_ok(cx, "System prompt saved");
                upd(cx, |s| { s.sp_status = "Saved \u{2014} applies to new conversations".into(); s.sp_tone.clear(); });
            }
            Err(e) => err(format!("Failed to save: {e}"), cx),
        }
    });
}

fn set_newline(cx: &mut App, i: i32) {
    local::update(|l| l.newline = if i == 1 { "ctrl".into() } else { "shift".into() });
    upd(cx, |s| s.newline_key = i);
}

fn autopark_commit(cx: &mut App, text: &str) {
    let (cur, min) = get(cx, |s| (s.autopark, s.autopark_min));
    let n = match rules::autopark(text, min) {
        Ok(n) => n,
        Err(e) => { c::toast_error(cx, &e); set_field(cx, "autopark", cur.to_string()); return; }
    };
    if n == cur { return; }
    post(cx, "/api/settings", json!({ "autoParkMinutes": n }), move |r, cx| match c::ok_or(r, "Could not save") {
        Ok(_) => {
            upd(cx, |s| { s.autopark = n; s.f.set("autopark", n.to_string()); });
            c::toast_ok(cx, &if n > 0 { format!("Idle panes close after {n} minutes") } else { "Idle panes stay open".into() });
        }
        Err(e) => { c::toast_error(cx, &e); set_field(cx, "autopark", cur.to_string()); }
    });
}

// ─── Notifications ───

fn channel_toggle(cx: &mut App, id: &str, on: bool) {
    let id2 = id.to_string();
    post(cx, "/api/notify/channels", json!({ "id": id, "enabled": on }), move |r, cx| match c::ok_or(r, "Could not change that") {
        Ok(_) => upd(cx, |s| for ch in s.channels.iter_mut() { if ch.id == id2 { ch.enabled = on; } }),
        Err(e) => c::toast_error(cx, &format!("Could not change that: {e}").replace("Could not change that: Could not change that", "Could not change that")),
    });
}

fn chat_save(cx: &mut App) {
    let id = field(cx, "chat-id");
    post(cx, "/api/notify/telegram/chat", json!({ "chatId": id }), |r, cx| match c::ok_or(r, "Could not save that") {
        Ok(_) => { c::toast_ok(cx, "Chat ID saved"); render(cx); }
        Err(e) => c::toast_error(cx, &e),
    });
}

fn push_test(cx: &mut App) {
    post(cx, "/api/push/test", json!({}), |r, cx| match c::ok_or(r, "Could not send") {
        Ok(_) => c::toast_ok(cx, "Test sent"),
        Err(e) => c::toast_error(cx, &e),
    });
}

fn notify_pref(cx: &mut App, key: &str, mode: &str) {
    if key.is_empty() || !valid_mode(mode) { return; }
    let (key, mode) = (key.to_string(), mode.to_string());
    let prefs = upd(cx, |s| { s.prefs.insert(key, Value::String(mode)); s.notify = notify_rows(&s.prefs); s.prefs.clone() });
    post(cx, "/api/settings", json!({ "notifyPrefs": prefs }), |r, cx| {
        if let Err(e) = r { c::toast_error(cx, &format!("Failed to save notification setting: {e}")); }
    });
}

// ─── Sign-in ───

fn method_toggle(cx: &mut App, method: &str, on: bool) {
    let m = method.to_string();
    let action = if on { "enable" } else { "disable" };
    crundi_shell::call(cx, { let m = m.clone(); move |api| api.post("/api/auth/config", &json!({ "action": action, "method": m })) }, move |r, cx| match c::ok_or(r, "Could not change that") {
        Ok(_) => {
            upd(cx, |s| match m.as_str() {
                "password" => s.pw_on = on,
                "telegram" => s.tg_on = on,
                "google" => s.google.v.on = on,
                "github" => s.github.v.on = on,
                _ => {}
            });
            c::toast_ok(cx, &format!("{} sign-in {}", method_label(&m), if on { "enabled" } else { "disabled" }));
        }
        // The server refuses to leave you with no way in: say so plainly.
        Err(e) => { c::toast_error(cx, &e); upd(cx, |_| {}); }
    });
}

fn set_password(cx: &mut App) {
    let pw = field(cx, "new-password");
    if pw.is_empty() { c::toast_error(cx, "Type a password first"); return; }
    crundi_shell::call(cx, move |api| api.post("/api/auth/config", &json!({ "action": "set-password", "password": pw })), |r, cx| match c::ok_or(r, "Could not set that") {
        Ok(v) => {
            upd(cx, |s| {
                s.f.set("new-password", "");
                // A new secret with every password: show it at once.
                s.enrol = c::s(&v, "totpSecret");
                s.pw_set = true;
                s.pw_on = true;
            });
            c::toast_ok(cx, "Password set \u{2014} add the new code to your authenticator");
        }
        Err(e) => c::toast_error(cx, &e),
    });
}

fn tg_save(cx: &mut App) {
    let body = get(cx, |s| json!({ "action": "telegram-save", "botToken": s.f.get("tg-token"), "allowedUsername": s.f.get("tg-user") }));
    crundi_shell::call(cx, move |api| api.post("/api/auth/config", &body), |r, cx| match c::ok_or(r, "Could not save that") {
        Ok(v) => {
            c::toast_ok(cx, if c::t(&v, "restartRequired") { "Saved. Restart required for the bot to use it." } else { "Saved" });
            render(cx);
        }
        Err(e) => c::toast_error(cx, &e),
    });
}

fn tg_clear(cx: &mut App) {
    c::confirm(cx, "Remove the Telegram bot?", "Remove the Telegram bot? Telegram sign-in and Telegram notifications both stop after the next restart.", "Remove", true, |cx| {
        crundi_shell::call(cx, |api| api.post("/api/auth/config", &json!({ "action": "telegram-clear" })), |r, cx| match c::ok_or(r, "Could not remove that") {
            Ok(_) => { c::toast_ok(cx, "Removed. Restart required."); render(cx); }
            Err(e) => c::toast_error(cx, &e),
        });
    });
}

fn oauth_save(cx: &mut App, k: &str) {
    let (a, b, d) = if k == "google" { ("google-id", "google-secret", "google-allowed") } else { ("github-id", "github-secret", "github-allowed") };
    let body = get(cx, |s| json!({ "action": "oauth-save", "provider": k, "clientId": s.f.get(a), "clientSecret": s.f.get(b), "allowed": s.f.get(d) }));
    crundi_shell::call(cx, move |api| api.post("/api/auth/config", &body), |r, cx| match c::ok_or(r, "Could not save that") {
        Ok(_) => { c::toast_ok(cx, "Saved. Run Test to check it works."); render(cx); }
        Err(e) => c::toast_error(cx, &e),
    });
}

fn oauth_clear(cx: &mut App, k: &str) {
    let k = k.to_string();
    let msg = format!("Remove {} sign-in and forget its client ID and secret?", oauth_label(&k));
    c::confirm(cx, "Remove sign-in?", &msg, "Remove", true, move |cx| {
        crundi_shell::call(cx, move |api| api.post("/api/auth/config", &json!({ "action": "oauth-clear", "provider": k })), |r, cx| match c::ok_or(r, "Could not remove that") {
            Ok(_) => { c::toast_ok(cx, "Removed"); render(cx); }
            Err(e) => c::toast_error(cx, &e),
        });
    });
}

fn oauth_copy(cx: &mut App, k: &str) {
    let url = get(cx, |s| s.data.pointer(&format!("/auth/callbackUrls/{k}")).and_then(|u| u.as_str()).unwrap_or("").to_string());
    if c::copy_text(cx, &url) { c::toast_ok(cx, "Copied") } else { c::toast_error(cx, "Select it and copy it by hand") }
}

fn oauth_line(cx: &mut App, k: &str, line: &str) {
    let line = line.to_string();
    upd(cx, |s| if k == "google" { s.google.test_line = line } else { s.github.test_line = line });
}

/// authOAuthTest: the server makes the provider's address, the system
/// browser opens it, and this asks every 1.5 s what came of it (10 minutes).
fn oauth_test(cx: &mut App, k: &str) {
    let k = k.to_string();
    let body = json!({ "provider": k, "purpose": "test", "origin": base(cx), "desktop": true });
    crundi_shell::call(cx, move |api| api.post("/api/auth/oauth/start", &body), move |r, cx| {
        let d = match c::ok_or(r, "Could not start the test") { Ok(d) => d, Err(e) => { c::toast_error(cx, &e); return; } };
        open_url(cx, &c::s(&d, "url"));
        oauth_line(cx, &k, "Waiting for the sign-in in the other window\u{2026}");
        let epoch = upd(cx, |s| { s.epoch += 1; let g = s.epoch; s.oauth_polls.insert(k.clone(), OAuthPoll { epoch: g, poll_id: c::s(&d, "pollId"), until: Instant::now() + Duration::from_secs(600), busy: false }); g });
        oauth_tick_later(cx, k, epoch);
    });
}

fn oauth_tick_later(cx: &mut App, k: String, epoch: u64) {
    c::after(cx, 1500, move |cx| {
        if get(cx, |s| s.oauth_polls.get(&k).map(|p| p.epoch) != Some(epoch)) { return; }
        oauth_tick(cx, &k);
        oauth_tick_later(cx, k, epoch);
    });
}

fn oauth_stop(cx: &mut App, k: &str, line: &str) {
    upd(cx, |s| s.oauth_polls.remove(k));
    if !line.is_empty() { oauth_line(cx, k, line); }
}

fn oauth_tick(cx: &mut App, k: &str) {
    let Some((pid, late)) = upd(cx, |s| s.oauth_polls.get_mut(k).filter(|p| !p.busy).map(|p| { p.busy = true; (p.poll_id.clone(), Instant::now() > p.until) })) else { return };
    if late { oauth_stop(cx, k, "The test timed out."); return; }
    let k = k.to_string();
    crundi_shell::call(cx, move |api| api.post("/api/auth/oauth/poll", &json!({ "pollId": pid })), move |r, cx| {
        upd(cx, |s| if let Some(p) = s.oauth_polls.get_mut(&k) { p.busy = false; });
        let p = r.unwrap_or(Value::Null);
        match c::s(&p, "status").as_str() {
            "done" => { oauth_stop(cx, &k, ""); c::toast_ok(cx, &format!("{} sign-in works ({})", oauth_label(&k), c::s(&p, "account"))); render(cx); }
            "failed" => {
                let e = c::s(&p, "error");
                let acct = c::s(&p, "account");
                let msg = format!("{}{}", if e.is_empty() { "The test did not pass".to_string() } else { e }, if acct.is_empty() { String::new() } else { format!(" (signed in as {acct})") });
                oauth_stop(cx, &k, &msg);
                c::toast_error(cx, &msg);
            }
            "expired" => oauth_stop(cx, &k, "The test expired."),
            _ => {}
        }
    });
}

// ─── Server and Claude Code ───

fn server_update(cx: &mut App, force: bool) {
    let force = force || get(cx, |s| s.srv_checked.map(|t| t.elapsed() > Duration::from_secs(60)).unwrap_or(true));
    if force { upd(cx, |s| s.srv_checked = Some(Instant::now())); }
    let path = if force { "/api/update/status?force=1" } else { "/api/update/status" };
    crundi_shell::call(cx, move |api| api.get(path), |r, cx| {
        let d = match c::ok_or(r, "Failed") {
            Ok(d) => d,
            Err(e) => { upd(cx, |s| { s.srv_error = e; s.srv_body = "error".into(); }); return; }
        };
        let v = server_view(&d, crundi_core::time::now_secs());
        let applying = v.applying;
        let start = upd(cx, |s| {
            s.srv = v;
            s.srv_body = "data".into();
            // While it updates the server restarts underneath: keep asking.
            if applying && !s.srv_polling { s.srv_polling = true; s.srv_poll += 1; Some(s.srv_poll) }
            else { if !applying { s.srv_polling = false; s.srv_poll += 1; } None }
        });
        if let Some(epoch) = start { server_poll(cx, epoch); }
    });
}

fn server_poll(cx: &mut App, epoch: u64) {
    c::after(cx, 5000, move |cx| {
        if get(cx, |s| s.srv_poll != epoch || !s.srv_polling) { return; }
        server_update(cx, false);
        server_poll(cx, epoch);
    });
}

fn server_channel(cx: &mut App, i: i32) {
    if get(cx, |s| s.srv.channel) == i { return; }
    let go = move |cx: &mut App| {
        let want = if i == 1 { "dev" } else { "production" };
        post(cx, "/api/update/channel", json!({ "channel": want }), move |r, cx| match c::ok_or(r, "Could not switch") {
            Ok(_) => { c::toast_ok(cx, if i == 1 { "Server follows dev releases" } else { "Server follows production releases" }); server_update(cx, false); }
            Err(e) => c::toast_error(cx, &e),
        });
    };
    if i == 1 { confirm_dev(cx, "this server", go); } else { go(cx); }
}

/// confirmDevChannel: leaving production is the one-way step.
fn confirm_dev(cx: &mut App, what: &str, go: impl FnOnce(&mut App) + 'static) {
    let msg = format!("Switch {what} to dev releases?\n\nDev builds are previews of what is coming and may be less stable.\n\nYou cannot go back to production until a production release newer than your dev build is published: Crundi never installs an older version. A newer production release is always offered on the dev channel too.");
    c::confirm(cx, "Switch to dev releases?", &msg, "Switch", false, go);
}

fn server_apply(cx: &mut App) {
    c::confirm(cx, "Update the server now?", "Update the server now?\n\nThis restarts Crundi when it finishes, which ends every running chat and terminal on it.", "Update", false, |cx| {
        upd(cx, |s| s.srv_body = "starting".into());
        crundi_shell::call(cx, |api| api.post("/api/update/apply", &json!({})), |r, cx| {
            match c::ok_or(r, "Could not start the update") {
                Ok(d) => c::toast_ok(cx, &format!("Updating to {}", c::s(&d, "version"))),
                Err(e) => c::toast_error(cx, &e),
            }
            server_update(cx, false);
        });
    });
}

fn restart_server(cx: &mut App) {
    c::confirm(cx, "Restart the server?", "Restart the server?\n\nThis ends every running chat and terminal on it. It comes back on its own in a few seconds.", "Restart", true, |cx| {
        crundi_shell::call(cx, |api| api.post("/api/restart", &json!({})), |r, cx| match r {
            Ok(v) if v.get("ok") == Some(&Value::Bool(false)) => c::toast_error(cx, &{ let e = c::s(&v, "error"); if e.is_empty() { "Could not restart".into() } else { e } }),
            // The connection dropping is the restart: not an error.
            _ => c::toast_ok(cx, "Restarting\u{2026}"),
        });
    });
}

fn claude_update(cx: &mut App, force: bool) {
    let path = if force { "/api/claude-update/status?force=1" } else { "/api/claude-update/status" };
    crundi_shell::call(cx, move |api| api.get(path), |r, cx| {
        let cl = match c::ok_or(r, "Failed") {
            Ok(d) => d.get("claude").cloned().unwrap_or(Value::Null),
            Err(e) => { upd(cx, |s| { s.cc_error = e; s.cc_body = "error".into(); }); return; }
        };
        let v = claude_view(&cl);
        upd(cx, |s| { s.cc = v; s.cc_body = "data".into(); });
    });
}

fn claude_apply(cx: &mut App) {
    let (version, beyond) = get(cx, |s| (s.cc.latest.clone(), s.cc.beyond));
    let v2 = version.clone();
    let go = move |cx: &mut App| {
        upd(cx, |s| s.cc_body = "installing".into());
        let body = json!({ "version": v2, "allowBeyondTested": beyond });
        crundi_shell::call(cx, move |api| api.post("/api/claude-update/apply", &body), |r, cx| {
            match c::ok_or(r, "Update failed") {
                Ok(d) => c::toast_ok(cx, &format!("Claude Code updated to {}", c::s(&d, "installed"))),
                Err(e) => c::toast_error(cx, &e),
            }
            claude_update(cx, true);
        });
    };
    if beyond {
        let msg = format!("Claude Code {version} is newer than the version this Crundi build was tested with.\n\nClaude Code is what every chat and terminal runs on, so a problem there affects all of them.\n\nInstall it anyway?");
        c::confirm(cx, "Install anyway?", &msg, "Install", false, go);
    } else { go(cx); }
}

fn whats_new(cx: &mut App) {
    // The web's openWhatsNew(true): the whole tour, or "Nothing new to show for this version." (extras').
    crate::extras::open_whatsnew(true, cx);
}

// ─── Session ───

fn sess_toggle(cx: &mut App) {
    let open = upd(cx, |s| { s.sess_open = !s.sess_open; s.sess_open });
    if open { sessions_load(cx); }
}

fn sessions_load(cx: &mut App) {
    upd(cx, |s| s.sess_state = "loading".into());
    crundi_shell::call(cx, |api| api.get("/api/auth/sessions"), |r, cx| match c::ok_or(r, "Failed") {
        Ok(d) => {
            let list = d.get("sessions").and_then(|s| s.as_array()).cloned().unwrap_or_default();
            let rows = session_rows(&list, crundi_core::time::now_secs());
            let others = rows.iter().filter(|r| !r.current).count() as i32;
            upd(cx, |s| { s.sessions = rows; s.sess_others = others; s.sess_state = if list.is_empty() { "empty" } else { "list" }.into(); });
        }
        Err(e) => upd(cx, |s| { s.sess_error = e; s.sess_state = "error".into(); }),
    });
}

fn sess_revoke(cx: &mut App, id: String) {
    if id.is_empty() { return; }
    post(cx, "/api/auth/sessions/revoke", json!({ "id": id }), |r, cx| {
        match c::ok_or(r, "Failed") { Ok(_) => c::toast_ok(cx, "Session revoked"), Err(e) => c::toast_error(cx, &e) }
        sessions_load(cx);
    });
}

fn sess_revoke_others(cx: &mut App) {
    post(cx, "/api/auth/sessions/revoke", json!({ "others": true }), |r, cx| {
        match c::ok_or(r, "Failed") {
            Ok(d) => {
                let n = d.get("revoked").and_then(|n| n.as_i64()).unwrap_or(0);
                c::toast_ok(cx, &if n > 0 { format!("Signed out {n} other session(s)") } else { "Nothing else was signed in".into() });
            }
            Err(e) => c::toast_error(cx, &e),
        }
        sessions_load(cx);
    });
}

// ─── Phone app ───

fn push_load(cx: &mut App) {
    upd(cx, |s| s.push_state = "checking".into());
    crundi_shell::call(cx, |api| api.get("/api/push/mobile"), |r, cx| match r {
        Ok(d) if d.get("ok") == Some(&Value::Bool(true)) => upd(cx, |s| {
            s.push_line = push_line(&d);
            let has_key = c::t(&d, "hasKey");
            let url = d.pointer("/contact/url").and_then(|u| u.as_str()).unwrap_or("").to_string();
            let tg = d.pointer("/contact/telegram").and_then(|u| u.as_str()).unwrap_or("").to_string();
            s.push_contact = if has_key || url.is_empty() { String::new() } else if !tg.is_empty() { format!("Message @{tg}") } else { "Ask for a key".into() };
            s.push_contact_url = url;
            s.push_error = c::s(&d, "lastError");
            s.push_has_key = has_key;
            s.f.set("push-key", "");
            s.relay_summary = relay_summary(&d);
            s.relay_custom = c::t(&d, "customRelay");
            s.f.set("relay-url", if s.relay_custom { c::s(&d, "relayUrl") } else { String::new() });
            s.push_state = "data".into();
        }),
        _ => upd(cx, |s| s.push_state = "none".into()),
    });
}

fn push_save(cx: &mut App) {
    let key = field(cx, "push-key").trim().to_string();
    if key.is_empty() { return; }
    crundi_shell::call(cx, move |api| api.post("/api/push/mobile/key", &json!({ "key": key })), |r, cx| match r {
        Ok(d) if d.get("ok") == Some(&Value::Bool(true)) => {
            if c::t(&d, "accepted") {
                let l = c::s(&d, "label");
                c::toast_ok(cx, &format!("Push key saved{}", if l.is_empty() { String::new() } else { format!(" ({l})") }));
            } else {
                let e = c::s(&d, "error");
                c::toast_error(cx, if e.is_empty() { "Saved, but the relay did not accept it" } else { &e });
            }
            push_load(cx);
        }
        Ok(d) => { let e = c::s(&d, "error"); c::toast_error(cx, if e.is_empty() { "Could not save the key" } else { &e }); }
        Err(_) => c::toast_error(cx, "Could not reach the server"),
    });
}

fn push_mobile_test(cx: &mut App) {
    crundi_shell::call(cx, |api| api.post("/api/push/mobile/test", &json!({})), |r, cx| {
        match r {
            Ok(d) if d.get("ok") == Some(&Value::Bool(true)) => {
                if c::t(&d, "sent") { c::toast_ok(cx, "Test notification sent") }
                else { let n = c::s(&d, "note"); c::toast_ok(cx, if n.is_empty() { "The key is good." } else { &n }) }
            }
            Ok(d) => { let e = c::s(&d, "error"); c::toast_error(cx, if e.is_empty() { "The test did not go through" } else { &e }) }
            Err(_) => { c::toast_error(cx, "Could not reach the server"); return; }
        }
        push_load(cx);
    });
}

fn push_remove(cx: &mut App) {
    crundi_shell::call(cx, |api| api.delete("/api/push/mobile/key"), |r, cx| match r {
        Ok(_) => { c::toast(cx, "Push key removed"); push_load(cx); }
        Err(_) => c::toast_error(cx, "Could not remove the key"),
    });
}

fn relay_save(cx: &mut App, url: String) {
    crundi_shell::call(cx, move |api| api.post("/api/push/mobile/relay", &json!({ "relayUrl": url })), |r, cx| match r {
        Ok(d) if d.get("ok") == Some(&Value::Bool(true)) => {
            if d.get("reachable") == Some(&Value::Bool(false)) {
                let e = c::s(&d, "error");
                c::toast_error(cx, if e.is_empty() { "Saved, but that relay did not answer" } else { &e });
            } else { c::toast_ok(cx, "Relay changed"); }
            push_load(cx);
        }
        Ok(d) => { let e = c::s(&d, "error"); c::toast_error(cx, if e.is_empty() { "Could not change the relay" } else { &e }); }
        Err(_) => c::toast_error(cx, "Could not reach the server"),
    });
}

/// crundi-phone.js connect(): one dialog at a time.
fn pair_start(cx: &mut App) {
    if get(cx, |s| s.pair_open) { return; }
    upd(cx, |s| s.pair_open = true);
    pair_begin(cx);
}

fn pair_begin(cx: &mut App) {
    let epoch = upd(cx, |s| {
        s.epoch += 1;
        s.pair = None;
        s.pair_again = false;
        s.pair_line = "Getting a code\u{2026}".into();
        s.pair_done = false;
        s.pair_small.clear();
        s.pair_image = None;
        s.pair_pic_text.clear();
        s.pair_swap.clear();
        s.pair_copy.clear();
        s.epoch
    });
    let origin = base(cx);
    crundi_shell::call(cx, move |api| api.post("/api/pair/start", &json!({ "origin": origin })), move |r, cx| {
        if !get(cx, |s| s.pair_open && s.epoch == epoch) { return; }
        let d = match r {
            Ok(d) if d.get("ok") == Some(&Value::Bool(true)) => d,
            Ok(d) => {
                let e = c::s(&d, "error");
                upd(cx, |s| { s.pair_line = if e.is_empty() { "Could not get a pairing code.".into() } else { e }; s.pair_again = true; });
                return;
            }
            Err(_) => { upd(cx, |s| { s.pair_line = "Could not reach the server.".into(); s.pair_again = true; }); return; }
        };
        let left = d.get("expiresIn").and_then(|n| n.as_i64()).unwrap_or(300);
        let done_ms = d.get("doneMs").and_then(|n| n.as_u64()).unwrap_or(1500);
        let pair = Pair {
            code: c::s(&d, "code"), link: c::s(&d, "link"), qr: c::s(&d, "qr"), mark: c::s(&d, "mark"), showing_qr: false,
            note: String::new(), warn: if c::t(&d, "insecure") { "This server is not using https. ".into() } else { String::new() },
            left, paired: false, epoch,
        };
        let has_link = !pair.link.is_empty();
        upd(cx, |s| {
            s.pair = Some(pair);
            s.pair_copy = if has_link { "Copy the pairing link instead".into() } else { String::new() };
            s.pair_line = "Open the Crundi app on your phone and point it here.".into();
        });
        pair_show(cx);
        pair_tick(cx, epoch);
        pair_poll(cx, epoch, done_ms);
    });
}

fn pair_live(cx: &mut App, epoch: u64) -> bool { get(cx, |s| s.pair.as_ref().is_some_and(|p| p.epoch == epoch && !p.paired && !p.code.is_empty())) }

fn pair_tick(cx: &mut App, epoch: u64) {
    c::after(cx, 1000, move |cx| {
        if !pair_live(cx, epoch) { return; }
        upd(cx, |s| if let Some(p) = s.pair.as_mut() { p.left = (p.left - 1).max(0); });
        pair_paint(cx);
        pair_tick(cx, epoch);
    });
}

fn pair_poll(cx: &mut App, epoch: u64, done_ms: u64) {
    c::after(cx, 1500, move |cx| {
        if !pair_live(cx, epoch) { return; }
        let code = get(cx, |s| s.pair.as_ref().map(|p| p.code.clone()).unwrap_or_default());
        crundi_shell::call(cx, move |api| api.post("/api/pair/status", &json!({ "code": code })), move |r, cx| {
            let Ok(s) = r else { return };
            if s.get("ok") != Some(&Value::Bool(true)) || !pair_live(cx, epoch) { return; }
            match c::s(&s, "state").as_str() {
                "paired" => pair_succeed(cx, s.get("device").cloned().unwrap_or(Value::Null), done_ms),
                "expired" => upd(cx, |st| {
                    if let Some(p) = st.pair.as_mut() { p.code.clear(); }
                    st.pair_image = None;
                    st.pair_pic_text = "Expired".into();
                    st.pair_line = "That code ran out before a phone used it.".into();
                    st.pair_small.clear();
                    st.pair_swap.clear();
                    st.pair_copy.clear();
                    st.pair_again = true;
                }),
                _ => {}
            }
        });
        pair_poll(cx, epoch, done_ms);
    });
}

/// The picture: the Crundi code, or the QR code (the server's finished SVG).
fn pair_show(cx: &mut App) {
    upd(cx, |s| {
        let Some(p) = s.pair.as_mut() else { return };
        let svg = if p.showing_qr || p.mark.is_empty() { p.qr.clone() } else { p.mark.clone() };
        let has_mark = !p.mark.is_empty();
        p.note = if has_mark { String::new() } else { "This address is too long for the Crundi code, so this is a QR code.".into() };
        let showing_qr = p.showing_qr;
        s.pair_image = (!svg.is_empty()).then(|| Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())));
        if s.pair_image.is_none() { s.pair_pic_text.clear(); }
        s.pair_swap = if !has_mark { String::new() } else if showing_qr { "Show the Crundi code".into() } else { "Show a QR code instead".into() };
    });
    pair_paint(cx);
}

fn pair_paint(cx: &mut App) {
    upd(cx, |s| {
        let Some(p) = s.pair.as_ref() else { return };
        if p.paired { return; }
        s.pair_small = format!("{}{}{}", if p.note.is_empty() { String::new() } else { format!("{} ", p.note) }, p.warn, expires_in(p.left));
    });
}

fn pair_succeed(cx: &mut App, device: Value, done_ms: u64) {
    upd(cx, |s| if let Some(p) = s.pair.as_mut() { p.paired = true; p.showing_qr = false; });
    pair_show(cx);
    let name = { let n = c::s(&device, "name"); if n.is_empty() { "Phone".to_string() } else { n } };
    let n2 = name.clone();
    upd(cx, |s| { s.pair_swap.clear(); s.pair_copy.clear(); s.pair_small.clear(); s.pair_line = format!("{n2} connected"); s.pair_done = true; });
    c::toast_ok(cx, &format!("{name} connected"));
    c::after(cx, done_ms + 1800, pair_close);
}

fn pair_swap(cx: &mut App) {
    upd(cx, |s| if let Some(p) = s.pair.as_mut() { p.showing_qr = !p.showing_qr; });
    pair_show(cx);
}

fn pair_copy(cx: &mut App) {
    let Some(link) = get(cx, |s| s.pair.as_ref().map(|p| p.link.clone())) else { return };
    if link.is_empty() { return; }
    let good = c::copy_text(cx, &link);
    upd(cx, |s| s.pair_copy = if good { "Link copied. Paste it in the app." } else { "Could not copy the link." }.into());
    c::after(cx, 2600, |cx| if get(cx, |s| s.pair.is_some()) { upd(cx, |s| s.pair_copy = "Copy the pairing link instead".into()); });
}

fn pair_close(cx: &mut App) {
    if !get(cx, |s| s.pair_open) { return; }
    let (p, sess) = upd(cx, |s| { s.epoch += 1; s.pair_open = false; (s.pair.take(), s.sess_open) });
    let mut paired = false;
    if let Some(p) = p {
        paired = p.paired;
        // A code left on a closed dialog is a login nobody is watching: take it back.
        if !p.code.is_empty() && !p.paired { post(cx, "/api/pair/cancel", json!({ "code": p.code }), |_, _| {}); }
    }
    // A new phone is a new signed-in device.
    if paired && sess { sessions_load(cx); }
}

// ─── Cru ───

fn voice_load(cx: &mut App) {
    crundi_shell::call(cx, |api| api.get("/api/voice/settings"), |r, cx| {
        let s = match r { Ok(s) if s.get("ok") == Some(&Value::Bool(true)) => s, _ => { upd(cx, |s| s.voice.shown = false); return; } };
        upd(cx, |st| voice_show(st, &s));
    });
}

fn nums(list: &[f64]) -> Vec<Value> { list.iter().map(|n| json!(n)).collect() }

fn voice_show(st: &mut St, s: &Value) {
    let f = |k: &str| s.get(k).and_then(|v| v.as_f64());
    let v = &mut st.voice;
    v.sel.clear();
    v.has_brain = !c::s(s, "brain").is_empty();
    v.sel.insert("brain", (vec!["Claude API (the key)".into(), "Claude Code (your subscription)".into()], if c::s(s, "brain") == "claude-code" { 1 } else { 0 }, vec![json!("api"), json!("claude-code")]));
    let models = s.get("models").and_then(|m| m.as_array()).cloned().unwrap_or_default();
    let model = c::s(s, "model");
    v.sel.insert("model", (models.iter().map(|m| c::s(m, "label")).collect(), models.iter().position(|m| c::s(m, "id") == model).map(|i| i as i32).unwrap_or(-1), models.iter().map(|m| json!(c::s(m, "id"))).collect()));
    let efforts = s.get("efforts").and_then(|e| e.as_array()).cloned().unwrap_or_default();
    let effort = c::s(s, "effort");
    v.has_effort = !efforts.is_empty() && !effort.is_empty();
    if v.has_effort {
        let cur = models.iter().find(|m| c::s(m, "id") == model).cloned().unwrap_or(Value::Null);
        let takes = cur.get("efforts").and_then(|e| e.as_array()).filter(|a| !a.is_empty()).cloned().unwrap_or(efforts);
        let ids: Vec<String> = takes.iter().map(|e| e.as_str().unwrap_or("").to_string()).collect();
        v.sel.insert("effort", (ids.iter().map(|e| effort_label(e)).collect(), ids.iter().position(|e| *e == effort).map(|i| i as i32).unwrap_or(-1), ids.iter().map(|e| json!(e)).collect()));
        v.effort_hint = effort_hint(&cur);
    }
    let mut pick = |key: &'static str, list: &[f64], cur: f64, label: &dyn Fn(f64) -> String| {
        let ch = choices(list, cur);
        let labels: Vec<String> = ch.iter().map(|n| label(*n)).collect();
        v.sel.insert(key, (labels, index_of(&ch, cur), nums(&ch)));
    };
    pick("cliIdleMinutes", &[5.0, 10.0, 15.0, 30.0, 60.0, 120.0], f("cliIdleMinutes").unwrap_or(30.0), &|n| format!("{} minutes idle", js_num(n)));
    pick("speechRate", &[0.85, 1.0, 1.15, 1.3, 1.5], f("speechRate").filter(|r| *r > 0.0).unwrap_or(1.0), &rate_label);
    if let Some(vol) = f("volume") { pick("volume", &[25.0, 40.0, 55.0, 70.0, 85.0, 100.0], vol, &|n| format!("{}%", js_num(n))); }
    pick("pauseMs", &[1500.0, 2000.0, 2500.0, 3000.0, 4000.0], f("pauseMs").filter(|p| *p > 0.0).unwrap_or(2000.0), &|n| format!("{} seconds", js_num(n / 1000.0)));
    let sclose = f("screenCloseSeconds").unwrap_or(0.0);
    if sclose > 0.0 { pick("screenCloseSeconds", &[30.0, 45.0, 60.0, 120.0, 300.0], sclose, &|n| format!("{} of silence", sec_label(n as i64))); }
    v.has_volume = s.get("volume").is_some();
    v.has_sclose = sclose > 0.0;
    v.has_admin = s.get("adminMode").is_some();
    v.admin = c::t(s, "adminMode");
    v.admin_hint = admin_hint(s);
    let away = s.get("away").filter(|a| a.is_object());
    let classes = s.get("awayClasses").and_then(|c| c.as_array()).cloned();
    v.has_away = away.is_some() && classes.is_some();
    let allow: Vec<String> = away.and_then(|a| a.get("allow")).and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    if let (Some(a), Some(cl)) = (away, classes) {
        v.away = c::t(a, "on");
        v.away_classes = cl.iter().map(|x| (c::s(x, "id"), c::s(x, "label"), allow.contains(&c::s(x, "id")))).collect();
    }
    v.spend = spend_line(s);
    v.akey_set = c::t(s, "anthropicKeySet");
    v.dkey_set = c::t(s, "deepgramKeySet");
    v.xkey_set = c::t(s, "anthropicAdminKeySet");
    v.shown = true;
    st.f.set("vs-limit", js_num(f("monthlyLimitUsd").unwrap_or(0.0)));
    st.f.set("vs-bday", js_num(f("billingDay").filter(|n| *n > 0.0).unwrap_or(1.0)));
    st.f.set("vs-dlimit", js_num(f("deepgramLimitUsd").unwrap_or(0.0)));
    st.f.set("vs-dday", js_num(f("deepgramBillingDay").filter(|n| *n > 0.0).unwrap_or(1.0)));
    for k in ["vs-akey", "vs-dkey", "vs-xkey"] { st.f.set(k, ""); }
}

/// Save a part of Cru's settings; the status line says how it went.
fn voice_save(cx: &mut App, body: Value, note: String, then: impl FnOnce(bool, &mut App) + 'static) {
    crundi_shell::call(cx, move |api| api.post("/api/voice/settings", &body), move |r, cx| {
        let good = matches!(&r, Ok(v) if v.get("ok") == Some(&Value::Bool(true)));
        let text = if good { note } else { match &r { Ok(v) if !c::s(v, "error").is_empty() => c::s(v, "error"), _ => "Not saved".into() } };
        upd(cx, |s| { s.voice.status = text; s.voice.status_bad = !good; });
        then(good, cx);
    });
}

fn voice_pick(cx: &mut App, field: &str, i: i32) {
    let Some((key, v)) = upd(cx, |s| {
        let (k, e) = s.voice.sel.iter_mut().find(|(k, _)| **k == field)?;
        let v = e.2.get(i.max(0) as usize).cloned()?;
        e.1 = i;
        Some((*k, v))
    }) else { return };
    let note = match key {
        "brain" => if v == "claude-code" { "Cru answers through Claude Code now".to_string() } else { "Cru answers through the Claude API now".into() },
        "model" => "Model saved. It applies from the next conversation.".into(),
        "effort" => "Thinking effort saved. It applies from the next answer.".into(),
        "speechRate" => format!("Cru talks at {}", rate_label(v.as_f64().unwrap_or(1.0))),
        "volume" => format!("Cru talks at {}%", js_num(v.as_f64().unwrap_or(0.0))),
        _ => "Saved".into(),
    };
    let mut b = Map::new();
    b.insert(key.to_string(), v);
    voice_save(cx, Value::Object(b), note, |_, _| {});
}

fn voice_admin(cx: &mut App, want: bool) {
    upd(cx, |s| s.voice.admin = want);
    let note = if want { "Admin mode is on: Cru can run any command, also on its own; spoken requests need your voice" } else { "Admin mode is off" };
    voice_save(cx, json!({ "adminMode": want }), note.into(), move |good, cx| if !good { upd(cx, |s| s.voice.admin = !want); });
}

fn voice_away(cx: &mut App, on: Option<bool>, class: Option<(String, bool)>) {
    let (was, now_on, allow) = upd(cx, |s| {
        let was = s.voice.away;
        if let Some(on) = on { s.voice.away = on; }
        if let Some((id, ch)) = &class { for r in s.voice.away_classes.iter_mut() { if r.0 == *id { r.2 = *ch; } } }
        let allow: Vec<String> = s.voice.away_classes.iter().filter(|r| r.2).map(|r| r.0.clone()).collect();
        (was, s.voice.away, allow)
    });
    let note = match on { Some(true) => "Cru may now act on its own, as ticked", Some(false) => "Cru asks before every change again", None => "Saved" };
    voice_save(cx, json!({ "away": { "on": now_on, "allow": allow } }), note.into(), move |good, cx| {
        if !good && on.is_some() { upd(cx, |s| s.voice.away = was); }
    });
}

fn voice_away_stop(cx: &mut App) {
    c::confirm(cx, "Stop everything?", "Stop everything Cru does on its own? Its tickers, triggers and jobs pause until switched on again.", "Stop everything", true, |cx| {
        post(cx, "/api/voice/away/stop", json!({ "everything": true }), |r, cx| match r {
            Ok(d) if d.get("ok") == Some(&Value::Bool(true)) => upd(cx, |s| {
                s.voice.away = false;
                s.voice.away_note = format!("Stopped: {} wake-ups paused, {} jobs paused", d.get("wakeupsPaused").and_then(|n| n.as_i64()).unwrap_or(0), d.get("jobsPaused").and_then(|n| n.as_i64()).unwrap_or(0));
                s.voice.away_bad = false;
            }),
            other => {
                let e = match &other { Ok(d) => c::s(d, "error"), Err(_) => String::new() };
                upd(cx, |s| { s.voice.away_note = if e.is_empty() { "Could not stop".into() } else { e }; s.voice.away_bad = true; });
            }
        });
    });
}

fn voice_save_spend(cx: &mut App) {
    let body = get(cx, |s| {
        let n = |k: &str| s.f.trim(k).parse::<f64>().unwrap_or(0.0);
        let day = |k: &str| { let d = s.f.trim(k).parse::<f64>().unwrap_or(0.0); if d > 0.0 { d } else { 1.0 } };
        json!({ "monthlyLimitUsd": n("vs-limit"), "deepgramLimitUsd": n("vs-dlimit"), "billingDay": day("vs-bday"), "deepgramBillingDay": day("vs-dday") })
    });
    post(cx, "/api/voice/settings", body, |r, cx| {
        let good = matches!(&r, Ok(v) if v.get("ok") == Some(&Value::Bool(true)));
        let e = match &r { Ok(v) => c::s(v, "error"), Err(_) => String::new() };
        upd(cx, |s| { s.voice.spend_note = if good { "Saved".into() } else if e.is_empty() { "Not saved".into() } else { e }; s.voice.spend_bad = !good; });
    });
}

fn voice_save_keys(cx: &mut App) {
    let body = get(cx, |s| {
        let mut b = Map::new();
        for (f, k) in [("vs-akey", "anthropicKey"), ("vs-dkey", "deepgramKey"), ("vs-xkey", "anthropicAdminKey")] {
            let v = s.f.trim(f);
            if !v.is_empty() { b.insert(k.into(), Value::String(v)); }
        }
        b
    });
    if body.is_empty() { upd(cx, |s| { s.voice.keys_note = "Type a key to change it".into(); s.voice.keys_bad = false; }); return; }
    post(cx, "/api/voice/settings", Value::Object(body), |r, cx| match r {
        Ok(v) if v.get("ok") == Some(&Value::Bool(true)) => { upd(cx, |s| s.voice.keys_note.clear()); voice_load(cx); }
        other => {
            let e = match &other { Ok(d) => c::s(d, "error"), Err(_) => String::new() };
            upd(cx, |s| { s.voice.keys_note = if e.is_empty() { "Not saved".into() } else { e }; s.voice.keys_bad = true; });
        }
    });
}

fn voice_reset(cx: &mut App) {
    post(cx, "/api/voice/reset", json!({}), |r, cx| {
        let good = matches!(&r, Ok(v) if v.get("ok") != Some(&Value::Bool(false)));
        upd(cx, |s| { s.voice.reset_note = if good { "Cru starts a fresh conversation" } else { "Could not reset" }.into(); s.voice.reset_bad = !good; });
    });
}

// ─── The Desktop App (this client) ───

fn desktop_show(cx: &mut App) {
    upd(cx, |s| {
        s.launch = local::launch();
        s.launch_hint = local::launch_hint();
        s.auto_update = local::auto_update();
        s.upd_channel = if local::channel() == "dev" { 1 } else { 0 };
    });
}

fn launch_toggle(cx: &mut App, on: bool) {
    match local::set_launch(on) {
        Ok(()) => upd(cx, |s| s.launch = local::launch()),
        Err(e) => c::toast_error(cx, &e),
    }
}

fn update_toggle(cx: &mut App, on: bool) {
    // update:setEnabled: saved, and on turning it on, checked at once.
    crundi_shell::lifecycle::set_auto_update(on);
    upd(cx, |s| s.auto_update = on);
    crundi_shell::lifecycle::updater(if on { "enabled-on" } else { "enabled-off" });
}

fn desktop_channel(cx: &mut App, i: i32) {
    if get(cx, |s| s.upd_channel) == i { return; }
    let go = move |cx: &mut App| {
        // update:setChannel: saved, the offer forgotten, checked again.
        crundi_shell::lifecycle::set_channel(i == 1);
        upd(cx, |s| s.upd_channel = i);
        crundi_shell::lifecycle::updater("channel");
    };
    if i == 1 { confirm_dev(cx, "the desktop app", go); } else { go(cx); }
}

fn update_check(cx: &mut App) {
    // update:check (the updater answers with its status, or a prompt).
    if !crundi_shell::lifecycle::updater("check") { c::toast(cx, "Update checks run in the installed app only."); }
}

/// update:install: Download & install, or Restart & install once downloaded.
fn update_install(_cx: &mut App) { crundi_shell::lifecycle::updater("install"); }

/// The signed-in server's address (kit::base).
fn base(cx: &App) -> String { crundi_shell::api(cx).map(|a| a.base().to_string()).unwrap_or_default() }

fn open_url(cx: &mut App, url: &str) {
    if !(url.starts_with("http://") || url.starts_with("https://")) { return; }
    let u = url.to_string();
    upd(cx, |s| s.data["opened"] = Value::String(u));
    c::open_url(url);
}

// ─── Fields' keys ───

/// Enter (or leaving the box) in a field: the control it submits.
fn on_key(k: &'static str, arg: &str, cx: &mut App) {
    let (text, blur) = c::key_arg(arg);
    match (k, blur) {
        ("chat-id", false) => chat_save(cx),
        ("new-password", false) => set_password(cx),
        ("push-key", false) => push_save(cx),
        // The web saves on change: Enter, or leaving the field.
        ("autopark", _) => { let t = text.to_string(); autopark_commit(cx, &t); }
        _ => {}
    }
}

// ─── Drawing ───

/// The page: the groups, their sections' heads and each open section's
/// parts as one virtual list of cached rows (crate::rows; F62): a wheel frame
/// builds again only the rows the list's edges cut, each a part of a section,
/// never a whole section. The state's changes lay the rows out again
/// (`relayout`: which sections are open and how many parts each has) and
/// remeasure them (`remeasure` keeps the scroll place).
pub struct Page { list: ListState, rows: Rows, slots: Rc<RefCell<Vec<Slot>>>, stale: Rc<Cell<bool>>, _subs: Vec<Subscription> }

/// One row of the page: a group's heading or a section's head (`Item`), or
/// part `j` of the `n` an open section's body has.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Slot { Item(usize), Part(usize, usize, usize) }

impl Page {
    fn new(st_e: &Entity<St>, sk_e: &Entity<crate::skills::Sk>, cx: &mut Context<Self>) -> Self {
        let again = |p: &mut Page, cx: &mut Context<Page>| { p.stale.set(true); cx.notify(); };
        let slots = Rc::new(RefCell::new((0..ITEMS.len()).map(Slot::Item).collect::<Vec<_>>()));
        let stale = Rc::new(Cell::new(true));
        Page {
            list: ListState::new(ITEMS.len(), ListAlignment::Top, T::SP_48 * 8.),
            rows: Self::make_rows(&slots, &stale, cx),
            slots, stale,
            _subs: vec![cx.observe(st_e, move |p, _, cx| again(p, cx)), cx.observe(sk_e, move |p, _, cx| again(p, cx))],
        }
    }

    fn make_rows(slots: &Rc<RefCell<Vec<Slot>>>, stale: &Rc<Cell<bool>>, cx: &mut Context<Self>) -> Rows {
        let n = slots.borrow().len();
        let (slots, stale, page) = (slots.clone(), stale.clone(), cx.entity_id());
        Rows::new(n, move |i, window, cx| {
            let (sl, open) = { let v = slots.borrow(); (v.get(i).copied(), matches!(v.get(i + 1), Some(Slot::Part(..)))) };
            let Some(sl) = sl else { return div().into_any_element() };
            let (el, fits) = slot_el(sl, open, window, cx);
            // A section's parts changed without the state saying so: lay out again.
            if !fits { stale.set(true); cx.defer(move |cx| cx.notify(page)); }
            el
        }, cx)
    }

    /// Which sections are open and how many parts each has: the list's rows
    /// (spliced where they changed, so the scroll place stays), built again.
    fn relayout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut new = vec![];
        for (i, it) in ITEMS.iter().enumerate() {
            new.push(Slot::Item(i));
            if let Item::Fold(k, _) = it {
                if is_open(cx, k) { let n = body_of(k, Some(usize::MAX), window, cx).len(); new.extend((0..n).map(|j| Slot::Part(i, j, n))); }
            }
        }
        let old = self.slots.borrow().clone();
        if old != new {
            let pre = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
            let most = old.len().min(new.len()) - pre;
            let suf = old.iter().rev().zip(new.iter().rev()).take(most).take_while(|(a, b)| a == b).count();
            self.list.splice(pre..old.len() - suf, new.len() - pre - suf);
            *self.slots.borrow_mut() = new;
        }
        if self.rows.len() != self.slots.borrow().len() { self.rows = Self::make_rows(&self.slots, &self.stale, cx); } else { self.rows.changed(cx); }
        self.list.remeasure();
    }
}

impl Render for Page {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("settings");
        ensure(cx);
        let (state, error) = get(cx, |s| (s.state.clone(), s.error.clone()));
        let body: AnyElement = match state.as_str() {
            "error" => w::page("settings-page").child(w::error_state("settings-retry", error, |_, _, cx| render(cx))).into_any_element(),
            "data" => {
                if self.stale.replace(false) { self.relayout(window, cx); }
                // The section just opened comes into view (the web's goto Sign-in scrolls to it).
                if let Some(k) = quiet(cx, |s| s.scroll_to.take()) {
                    if let Some(i) = ITEMS.iter().position(|it| matches!(it, Item::Fold(x, _) if *x == k)) {
                        if let Some(ix) = self.slots.borrow().iter().position(|s| *s == Slot::Item(i)) { self.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(0.) }); }
                    }
                }
                if quiet(cx, |s| std::mem::take(&mut s.reveal_skill)) {
                    if let Some(y) = crate::skills::open_row_y() { self.list.scroll_by(y - self.list.viewport_bounds().origin.y - T::SP_16); }
                }
                if self.rows.take_grew() { self.list.remeasure(); }
                list(self.list.clone(), cx.processor(|p: &mut Page, i: usize, _, _| p.rows.el(i)))
                    .size_full().p(T::SP_16).bg(T::bg()).into_any_element()
            }
            _ => w::page("settings-page").child(w::loading("Loading\u{2026}")).into_any_element(),
        };
        page_with(window, cx, body)
    }
}

/// The page's rows, in the web's groups (organizeSettings; the Desktop App
/// under "More", at the end).
#[derive(Clone, Copy)]
enum Item { Group(&'static str), Fold(&'static str, &'static str) }

const ITEMS: &[Item] = &[
    Item::Group("CRUNDI"), Item::Fold("server", "Server"), Item::Fold("cc", "Claude Code"), Item::Fold("whatsnew", "What\u{2019}s new"),
    Item::Group("CLAUDE"), Item::Fold("sysprompt", "System prompt"), Item::Fold("skills", "Skills"), Item::Fold("usage", "Usage"),
    Item::Group("CRU"), Item::Fold("voice", "Voice assistant"),
    Item::Group("NOTIFICATIONS"), Item::Fold("channels", "Notification Channels"), Item::Fold("notify", "Notification Settings"),
    Item::Group("ACCESS"), Item::Fold("signin", "Sign-in"), Item::Fold("phone", "Phone app"), Item::Fold("session", "Session"),
    Item::Group("THIS SERVER"), Item::Fold("config", "Configuration"), Item::Fold("terminal", "Terminal"), Item::Fold("backup", "Backup & restore"),
    Item::Group("MORE"), Item::Fold("desktop", "Desktop App"),
];

/// A section's body (drawn only while it is open), as its parts.
fn body_of(key: &str, want: Want, window: &mut Window, cx: &mut App) -> Parts {
    match key {
        "server" => server_body(want, cx),
        "cc" => claude_body(want, cx),
        "whatsnew" => Parts::one(row().gap(T::SP_12)
            .child(div().flex_1().min_w_0().text_size(T::TEXT_12).text_color(T::text_muted()).child("A short tour of what the latest update added. It shows by itself once after each update."))
            .child(w::button("set-whatsnew", "Show it again").on_click(|_, _, cx| whats_new(cx)))),
        "sysprompt" => sysprompt_body(want, window, cx),
        "skills" => crate::skills::skills_section(want, window, cx),
        "usage" => crate::skills::usage_section(want, window, cx),
        "voice" => voice_body(want, window, cx),
        "channels" => channels_body(want, window, cx),
        "notify" => notify_body(want, cx),
        "signin" => signin_body(want, window, cx),
        "phone" => phone_body(want, window, cx),
        "session" => session_body(want, cx),
        "config" => config_body(want, window, cx),
        "terminal" => terminal_body(want, window, cx),
        "backup" => crate::skills::backup_section(want, window, cx),
        _ => desktop_body(want, cx),
    }
}

/// One row of the page (and the page's gap under it, but the last), and
/// whether a part's section still has the parts the row was laid out for.
/// A section's card is drawn in slices: its head the top (or all of it when
/// closed), each part the sides, the last part the bottom.
fn slot_el(sl: Slot, open: bool, window: &mut Window, cx: &mut App) -> (AnyElement, bool) {
    let gap = |i: usize, last: bool, el: AnyElement| div().w_full().when(last && i + 1 < ITEMS.len(), |d| d.pb(T::SP_12)).child(el).into_any_element();
    match sl {
        Slot::Item(i) => (match ITEMS.get(i) {
            Some(Item::Group(t)) => gap(i, true, group(t, i == 0).into_any_element()),
            Some(Item::Fold(k, title)) if open => gap(i, false, slice(0, 2, T::R_FIELD, T::border(), Some(T::bg_secondary())).flex_none()
                .child(fold_head(k, title, true)).into_any_element()),
            Some(Item::Fold(k, title)) => gap(i, true, fold(k, title, is_open(cx, k), || div().into_any_element()).into_any_element()),
            None => div().into_any_element(),
        }, true),
        Slot::Part(i, j, n) => {
            let Some(Item::Fold(k, _)) = ITEMS.get(i) else { return (div().into_any_element(), true) };
            let body = body_of(k, Some(j), window, cx);
            let fits = body.len() == n;
            let (el, g) = body.take(j).unwrap_or_else(|| (div().into_any_element(), px(0.)));
            let last = j + 1 == n;
            (gap(i, last, slice(j + 1, n + 1, T::R_FIELD, T::border(), Some(T::bg_secondary())).px(T::SP_16).pt(g).when(last, |d| d.pb(T::SP_16))
                .child(el).into_any_element()), fits)
        }
    }
}

/// A hint with its link at the end, inline (the web's `… <a>Open</a>` in the paragraph).
fn hint_link(id: impl Into<ElementId>, text: &str, link: &str, after: &str, url: &'static str) -> Div {
    let full: SharedString = format!("{text} {link}{after}").into();
    let at = text.len() + 1..text.len() + 1 + link.len();
    let st = StyledText::new(full.clone()).with_highlights(vec![(at.clone(), HighlightStyle { color: Some(T::accent_hover()), ..Default::default() })]);
    div().text_size(T::TEXT_11).text_color(T::text_muted())
        .child(InteractiveText::new(id, st).on_click(vec![at], move |_, _, cx| open_url(cx, url)))
}
fn hint(text: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_11).text_color(T::text_muted()).child(text.into()) }
fn label(text: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(text.into()) }
fn sub_head(text: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child(text.into()) }
fn status(text: impl Into<SharedString>, tone: &str) -> Div {
    div().text_size(T::TEXT_12).text_color(match tone { "warn" => T::yellow(), "error" => T::red(), "ok" => T::green(), _ => T::text_muted() }).child(text.into())
}
fn note(text: impl Into<SharedString>, bad: bool) -> Div { status(text, if bad { "error" } else { "" }) }
fn row() -> Div { div().flex().items_center().gap(T::SP_8) }
fn col(gap: Pixels) -> Div { div().flex().flex_col().gap(gap) }
/// An element id from a name and a key.
fn eid(a: &str, b: &str) -> ElementId { ElementId::Name(format!("{a}-{b}").into()) }
fn hairline() -> Div { div().h(px(1.)).w_full().bg(T::border()) }

/// A group's heading (.set-group): small, upper case, muted, wide.
pub fn group(text: &str, first: bool) -> Div {
    div().pl(T::SP_2).when(!first, |d| d.pt(T::SP_8)).text_size(T::TEXT_11).text_color(T::text_muted()).child(text.to_string())
}

/// A section folded under its title (.info-section.set-fold): the head (the
/// title and a chevron, down when open) opens and closes it; the body is
/// drawn only while open, so its height, arrow and body follow the one flag.
pub fn fold(key: &'static str, title: &str, open: bool, body: impl FnOnce() -> AnyElement) -> Div {
    div().flex().flex_col().flex_none().bg(T::bg_secondary()).rounded(T::R_FIELD).border_1().border_color(T::border())
        .child(fold_head(key, title, open))
        .when(open, |d| d.child(div().flex().flex_col().gap(T::SP_12).px(T::SP_16).pb(T::SP_16).child(body())))
}

/// A section's head: the title and a chevron (down when open); a click opens or closes it.
fn fold_head(key: &'static str, title: &str, open: bool) -> Stateful<Div> {
    div().id(eid("set-fold", key)).flex().items_center().gap(T::SP_8).px(T::SP_16).py(T::SP_12).cursor_pointer()
        .text_color(T::text()).hover(|d| d.text_color(T::accent()))
        .child(div().flex_1().min_w_0().text_size(T::TEXT_14).font_weight(T::W_MEDIUM).child(title.to_string()))
        .child(w::icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }, T::ICON_SM))
        .on_click(move |_, _, cx| { let on = !is_open(cx, key); set_open(cx, key, on); })
}

/// A switch row (.seg-pref-row): the title, its hint and extras on the left, the switch on the right.
fn toggle_row(id: impl Into<ElementId>, title: &str, mark: Option<&'static [u8]>, hint_text: &str, on: bool, enabled: bool, extra: Option<AnyElement>, toggled: impl Fn(bool, &mut App) + 'static) -> Div {
    let title_row = row().gap(T::SP_6)
        .children(mark.map(|m| img(Arc::new(Image::from_bytes(ImageFormat::Svg, m.to_vec()))).size(T::ICON_SM)))
        .child(div().flex_1().min_w_0().text_size(T::TEXT_14).text_color(T::text()).child(title.to_string()));
    div().flex().gap(T::SP_16)
        .child(col(T::SP_4).flex_1().min_w_0().child(title_row).when(!hint_text.is_empty(), |d| d.child(hint(hint_text.to_string()))).children(extra))
        .child(div().flex_none().opacity(if enabled { 1.0 } else { T::SETTINGS_O_DIM })
            .child(w::switch(id, on, move |v, _, cx| if enabled { toggled(*v, cx) } else { upd(cx, |_| {}); })))
}

/// A row with a title and sub line on the left and its controls on the right.
fn info_row(title: &str, sub: &str, controls: Vec<AnyElement>) -> Div {
    div().flex().items_center().gap(T::SP_12)
        .child(col(T::SP_2).flex_1().min_w_0()
            .when(!title.is_empty(), |d| d.child(div().text_size(T::TEXT_14).text_color(T::text()).child(title.to_string())))
            .when(!sub.is_empty(), |d| d.child(hint(sub.to_string()))))
        .child(row().flex_none().children(controls))
}

fn input_box(e: &Entity<w::InputState>) -> AnyElement { w::input(e).into_any_element() }
fn mono_box(e: &Entity<w::InputState>) -> AnyElement { div().font_family(T::MONO).child(w::input(e)).into_any_element() }

const GOOGLE: &[u8] = include_bytes!("../assets/settings-icons/google.svg");
const GITHUB: &[u8] = include_bytes!("../assets/settings-icons/github.svg");
const TELEGRAM: &[u8] = include_bytes!("../assets/settings-icons/telegram.svg");

/// Change: the next key pressed with its modifiers becomes the shortcut (the
/// page has the focus and takes the keys until then; Esc cancels).
fn ptt_begin(window: &mut Window, cx: &mut App) {
    upd(cx, |s| { s.ptt_capturing = true; s.ptt_status = "Press the keys you want (Esc to cancel)"; });
    if let Some(f) = get(cx, |s| s.ptt_focus.clone()) { window.focus(&f, cx); }
}

fn ptt_key(e: &KeyDownEvent, _: &mut Window, cx: &mut App) {
    if !get(cx, |s| s.ptt_capturing) { return; }
    match crate::ptt::capture(&e.keystroke) {
        crate::ptt::Capture::Wait => {}
        crate::ptt::Capture::Cancel => upd(cx, |s| { s.ptt_capturing = false; s.ptt_status = ""; }),
        crate::ptt::Capture::Set(p) => { crate::ptt::set(Some(&p)); upd(cx, |s| { s.ptt_capturing = false; s.ptt_status = "Saved on this device"; }); }
    }
    cx.stop_propagation();
}

fn page_with(window: &mut Window, cx: &mut App, body: AnyElement) -> AnyElement {
    let overlay = pair_dialog(window, cx);
    let restore = crate::skills::login_restore(window, cx);
    let fh = match get(cx, |s| s.ptt_focus.clone()) { Some(f) => f, None => { let f = cx.focus_handle(); quiet(cx, |s| s.ptt_focus = Some(f.clone())); f } };
    div().relative().size_full().flex().flex_col().min_h_0().track_focus(&fh).on_key_down(ptt_key)
        .child(body)
        .children(overlay)
        .children(restore)
        .into_any_element()
}

fn server_body(want: Want, cx: &mut App) -> Parts {
    let s = st(cx);
    match s.srv_body.as_str() {
        "checking" => return Parts::one(hint("Checking for updates\u{2026}").text_size(T::TEXT_13)),
        "starting" => return Parts::one(hint("Starting the update\u{2026}").text_size(T::TEXT_13)),
        "error" => return Parts::one(hint(format!("Could not check for updates: {}", s.srv_error)).text_size(T::TEXT_13)),
        _ => {}
    }
    let v = &s.srv;
    let mut acts: Vec<AnyElement> = vec![];
    match v.act.as_str() {
        "applying" => acts.push(status("Working\u{2026}", "").into_any_element()),
        "update" => {
            acts.push(w::button("srv-check", "Check").on_click(|_, _, cx| server_update(cx, true)).into_any_element());
            acts.push(w::primary("srv-apply", format!("Update to {}", v.latest)).on_click(|_, _, cx| server_apply(cx)).into_any_element());
        }
        _ => acts.push(w::button("srv-check", "Check now").on_click(|_, _, cx| server_update(cx, true)).into_any_element()),
    }
    let notes = v.notes_url.clone();
    parts(T::SP_12).wanting(want)
        .part(|| info_row(&format!("Version {}", v.version), &v.sub, acts))
        .part(|| info_row("Release channel", "Dev gets preview builds early. A newer production release is offered either way.",
            vec![w::segmented("srv-channel", &["Production", "Dev"], v.channel.max(0) as usize, |i, _, cx| server_channel(cx, *i as i32)).into_any_element()]))
        .when(!v.dev_note.is_empty(), |d| d.part(|| hint(v.dev_note.clone())))
        .when(!v.blocker.is_empty(), |d| d.part(|| hint(v.blocker.clone()).text_color(T::yellow())))
        .when(!notes.is_empty(), |d| d.part(|| row().child(w::link("srv-notes", "Release notes").on_click(move |_, _, cx| open_url(cx, &notes)))))
        .when(!v.log.is_empty(), |d| d.part(|| div().p(T::SP_8).bg(T::bg()).rounded(T::R_CONTROL).border_1().border_color(T::border())
            .font_family(T::MONO).text_size(T::TEXT_10).text_color(T::text_muted()).child(v.log.clone())))
        .when(v.can_restart, |d| d.part(|| hairline()).child(info_row("", "Settings like the Telegram token and TLS only take effect after a restart.",
            vec![w::button("srv-restart", "Restart server").on_click(|_, _, cx| restart_server(cx)).into_any_element()])))
}

fn claude_body(want: Want, cx: &mut App) -> Parts {
    let s = st(cx);
    match s.cc_body.as_str() {
        "checking" => return Parts::one(hint("Checking\u{2026}").text_size(T::TEXT_13)),
        "installing" => return Parts::one(hint("Installing\u{2026} this can take a minute.").text_size(T::TEXT_13)),
        "error" => return Parts::one(hint(format!("Could not check: {}", s.cc_error)).text_size(T::TEXT_13)),
        _ => {}
    }
    let v = &s.cc;
    let mut acts: Vec<AnyElement> = vec![];
    match v.act.as_str() {
        "applying" => acts.push(status("Working\u{2026}", "").into_any_element()),
        "update" => {
            acts.push(w::button("cc-check", "Check").on_click(|_, _, cx| claude_update(cx, true)).into_any_element());
            // Past the tested version: another colour, so the two decisions never look alike.
            // (The web's warn style: yellow edge and text on its dim fill; no icon.)
            let b = if v.beyond { w::button("cc-apply", format!("Update to {}", v.latest)).border_1().border_color(T::yellow()).text_color(T::yellow()).bg(T::yellow_dim()) }
                else { w::primary("cc-apply", format!("Update to {}", v.latest)) };
            acts.push(b.on_click(|_, _, cx| claude_apply(cx)).into_any_element());
        }
        _ => acts.push(w::button("cc-check", "Check now").on_click(|_, _, cx| claude_update(cx, true)).into_any_element()),
    }
    parts(T::SP_8).wanting(want)
        .part(|| info_row(&format!("Version {}", v.version), &v.sub, acts))
        // One line: the tested version, then its warning in yellow (a span in the web).
        .when(!v.tested.is_empty(), |d| {
            let warn = !v.tested_warn.is_empty();
            let text: SharedString = if warn { format!("{} {}", v.tested, v.tested_warn).into() } else { v.tested.clone().into() };
            let from = v.tested.len() + 1;
            let st = StyledText::new(text.clone()).with_highlights(if warn { vec![(from..text.len(), HighlightStyle { color: Some(T::yellow()), ..Default::default() })] } else { vec![] });
            d.child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(st))
        })
        .when(!v.blocker.is_empty(), |d| d.part(|| hint(v.blocker.clone()).text_color(T::yellow())))
        .when(!v.sudo.is_empty(), |d| d.part(|| hint(v.sudo.clone())))
        .when(!v.last.is_empty(), |d| d.part(|| hint(v.last.clone())))
}

fn sysprompt_body(want: Want, window: &mut Window, cx: &mut App) -> Parts {
    let all = c::area::<St>(window, cx, "sysprompt", "e.g. Prefer small commits. Ask before installing anything globally.", 5, None);
    let proj = c::area::<St>(window, cx, "proj-sysprompt", "e.g. This repo deploys on merge to main. Never push directly.", 5, None);
    let s = st(cx);
    parts(T::SP_12).wanting(want)
        .part(|| hint("Added to every chat and terminal Crundi starts, after its own description of this machine and the tools available here. Scheduled jobs deliberately get only that description, so an edit here cannot change what an unattended job does."))
        .part(|| col(T::SP_4).child(label("Everywhere")).child(w::textarea(&all)).child(hint("Applies to every project.")))
        .when(!s.proj_name.is_empty(), |d| d.part(|| col(T::SP_4).child(label(format!("Just {}", s.proj_name))).child(w::textarea(&proj))
            .child(hint("Read last, so it wins where it disagrees with the box above."))))
        .when(s.proj_name.is_empty(), |d| d.part(|| hint("Open a project to give it its own additional prompt.")))
        .part(|| hint("Takes effect for new conversations. Claude Code records a conversation\u{2019}s system prompt the first time it runs and replays that recording on every later turn and on resume, so a chat already open keeps the wording it started with."))
        .part(|| row().child(w::primary("sp-save", "Save prompts").on_click(|_, _, cx| save_prompts(cx))).child(status(s.sp_status.clone(), &s.sp_tone)))
}

fn vs_select(key: &'static str, label_text: &str, hint_text: &str, s: &St) -> Div {
    let (labels, on) = s.voice.sel.get(key).map(|e| (e.0.clone(), e.1)).unwrap_or_default();
    col(T::SP_4).child(label(label_text.to_string()))
        .child(div().w(T::SETTINGS_SELECT_W).child(w::select(eid("vs-sel", key), labels.into_iter().map(SharedString::from).collect(), (on >= 0).then_some(on as usize), "", false,
            move |i, _, cx| voice_pick(cx, key, i as i32))))
        .when(!hint_text.is_empty(), |d| d.child(hint(hint_text.to_string())))
}

fn vs_field(e: &Entity<w::InputState>, label_text: &str, hint_text: &str) -> Div {
    col(T::SP_4).child(label(label_text.to_string())).child(div().w(T::SETTINGS_SELECT_W).child(w::input(e)))
        .when(!hint_text.is_empty(), |d| d.child(hint(hint_text.to_string())))
}

fn voice_body(want: Want, window: &mut Window, cx: &mut App) -> Parts {
    let keep = "Leave empty to keep the current key";
    let lim = c::line::<St>(window, cx, "vs-limit", "", false, None);
    let bday = c::line::<St>(window, cx, "vs-bday", "", false, None);
    let dlim = c::line::<St>(window, cx, "vs-dlimit", "", false, None);
    let dday = c::line::<St>(window, cx, "vs-dday", "", false, None);
    let ak = c::line::<St>(window, cx, "vs-akey", keep, true, None);
    let dk = c::line::<St>(window, cx, "vs-dkey", keep, true, None);
    let xk = c::line::<St>(window, cx, "vs-xkey", keep, true, None);
    // On this device (the web's, kept per device): the push-to-talk shortcut.
    // (The web's: the key shown as it is, Change and Reset, and the status line beside them.)
    let status = st(cx).ptt_status;
    let device = col(T::SP_8).child(sub_head("On this device"))
        .child(row().flex_wrap()
            .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("Push to talk"))
            .child(w::kbd(crate::ptt::label(&crate::ptt::get())).id("vs-ptt-key"))
            .child(w::button("vs-ptt-change", "Change").on_click(|_, window, cx| ptt_begin(window, cx)))
            .child(w::button("vs-ptt-reset", "Reset").on_click(|_, _, cx| { crate::ptt::set(None); upd(cx, |s| { s.ptt_capturing = false; s.ptt_status = ""; }); }))
            .when(!status.is_empty(), |d| d.child(div().id("vs-ptt-status").text_size(T::TEXT_12).text_color(T::text_muted()).child(status))))
        .children(crate::compat::settings_parts(window, cx, "voice-device"));
    let s = st(cx);
    let v = &s.voice;
    let mut d = parts(T::SP_10).wanting(want).part(|| hint("Click the mic in the top bar, hover over it, or hold the shortcut anywhere to talk to Cru, the voice assistant. What follows applies to Cru on every device."));
    if !v.shown { return d.part(|| device); }
    let set = |b: bool| if b { "set" } else { "not set" };
    d = d.part(|| sub_head("Brain and model"))
        .when(v.has_brain, |d| d.part(|| vs_select("brain", "Brain", "An experiment: the same tools, rules and notebook either way. Each keeps its own conversation.", s)))
        .part(|| vs_select("model", "Model", "Haiku is the quickest and cheapest. A change applies from the next conversation.", s))
        .when(v.has_effort, |d| d.part(|| vs_select("effort", "Thinking effort", &v.effort_hint, s)))
        .when(v.has_brain, |d| d.part(|| vs_select("cliIdleMinutes", "Stop Claude Code after", "Your next message starts it again and carries on the same conversation.", s)))
        .part(|| sub_head("Voice and listening"))
        .part(|| vs_select("speechRate", "Talking speed", "", s))
        .when(v.has_volume, |d| d.part(|| vs_select("volume", "Volume", "How loud Cru talks, everywhere. Cru can change it too: say louder or quieter.", s)))
        .part(|| vs_select("pauseMs", "Pause before sending", "How long you may pause before what you said is sent.", s))
        .when(v.has_sclose, |d| d.part(|| vs_select("screenCloseSeconds", "Close the voice screen after", "On the phone, the web and the PC. Hey Cru opens it again.", s)))
        .when(v.has_admin, |d| d.part(|| col(T::SP_4)
            .child(row().gap(T::SP_6).child(w::check("vs-admin", "", v.admin, |on, _, cx| voice_admin(cx, *on))).child(w::icon(IconName::ShieldAlert, T::ICON_SM))
                .child(div().text_size(T::TEXT_13).text_color(T::text()).child("Admin mode")))
            .child(hint(v.admin_hint.clone()))))
        .when(v.has_away, |d| d.part(|| col(T::SP_4)
            .child(row().gap(T::SP_6).child(w::check("vs-away", "", v.away, |on, _, cx| voice_away(cx, Some(*on), None))).child(w::icon(IconName::Zap, T::ICON_SM))
                .child(div().text_size(T::TEXT_13).text_color(T::text()).child("Cru on its own")))
            .child(hint("When nobody is talking to it (a wake-up, the heartbeat, a job it took on), Cru may do what is ticked below without asking. Anything else comes to your phone to approve or decline. With chats ticked, Cru also closes a chat without asking, even while you talk to it, once it has read what the chat is doing. Cru can switch this off, never on."))
            .child(col(T::SP_4).pl(T::SP_24).opacity(if v.away { 1.0 } else { T::SETTINGS_O_DIM }).children(v.away_classes.iter().enumerate().map(|(i, (id, l, on))| {
                let id = id.clone();
                w::check(("vs-class", i), l.clone(), *on, move |v, _, cx| voice_away(cx, None, Some((id.clone(), *v)))).into_any_element()
            })))
            .child(row().child(w::danger("vs-stop", "Stop everything").icon(IconName::Pause).on_click(|_, _, cx| voice_away_stop(cx))).child(note(v.away_note.clone(), v.away_bad)))
            .child(hint("Switches this off, pauses every ticker and trigger Cru keeps (the heartbeat too), drops its questions and pauses its jobs."))))
        .part(|| sub_head("Spending"))
        .part(|| hint(v.spend.clone()))
        .part(|| vs_field(&lim, "Claude API monthly limit (US$)", "Cru stops answering through the API at it."))
        .part(|| vs_field(&bday, "Claude API billing day", "The day of the month the API is billed; its spend restarts then."))
        .part(|| vs_field(&dlim, "Deepgram monthly limit (US$)", "At it, the device voice and recogniser take over."))
        .part(|| vs_field(&dday, "Deepgram billing day", ""))
        .part(|| row().child(w::button("vs-spend", "Save spending").on_click(|_, _, cx| voice_save_spend(cx))).child(note(v.spend_note.clone(), v.spend_bad)))
        .part(|| sub_head("Keys"))
        .part(|| vs_field(&ak, &format!("Claude API key ({})", set(v.akey_set)), ""))
        .part(|| vs_field(&dk, &format!("Deepgram API key ({})", set(v.dkey_set)), "Listening and speaking."))
        .part(|| vs_field(&xk, &format!("Anthropic Admin API key, optional ({})", set(v.xkey_set)), "Only to show the whole account usage."))
        .part(|| row().child(w::button("vs-keys", "Save keys").on_click(|_, _, cx| voice_save_keys(cx))).child(note(v.keys_note.clone(), v.keys_bad)))
        .part(|| sub_head("Conversation and notebook"))
        .part(|| row().child(w::button("vs-reset", "Start a fresh conversation").on_click(|_, _, cx| voice_reset(cx))).child(note(v.reset_note.clone(), v.reset_bad)))
        .part(|| hint("Nothing is lost: memory search keeps every earlier turn."))
        .when(!v.status.is_empty(), |d| d.part(|| note(v.status.clone(), v.status_bad)))
        .part(|| device);
    d
}

fn channels_body(want: Want, window: &mut Window, cx: &mut App) -> Parts {
    let chat = c::line::<St>(window, cx, "chat-id", "Chat ID (filled in when you send /start)", false, Some(on_key));
    let s = st(cx);
    let mut d = parts(T::SP_12).wanting(want).child(hint("Which of these get told. Everything for you lands in the Crundi inbox first, and it is always on; turn on as many of the others as you like, or none at all. The table below decides which events are worth telling you about."));
    for (i, ch) in s.channels.iter().enumerate() {
        let extra: Option<AnyElement> = if ch.id == "webpush" && ch.available {
            // Web Push is a browser's: this client offers only the test (no "Allow in this browser").
            Some(row().pt(T::SP_4).child(w::button("push-test", "Send a test").on_click(|_, _, cx| push_test(cx))).into_any_element())
        } else if ch.telegram {
            let tone = match s.tgc_tone.as_str() { "ok" => T::green(), "warn" => T::yellow(), "error" => T::red(), _ => T::text_muted() };
            Some(col(T::SP_8).pt(T::SP_4)
                .child(row().gap(T::SP_6).text_size(T::TEXT_12)
                    .child(div().text_color(T::text_secondary()).child("Bot:")).child(w::dot(tone))
                    .child(div().text_color(T::text_secondary()).child(s.tgc_state.clone()))
                    .child(div().text_color(T::text_muted()).child(format!("\u{b7} token {},", if s.tgc_token_set { "set" } else { "not set" })))
                    .child(w::link("goto-signin", if s.tgc_token_set { "edit under Sign-in" } else { "set it up under Sign-in" }).on_click(|_, _, cx| { set_open(cx, "signin", true); })))
                .child(row().child(div().w(T::SETTINGS_CHAT_W).child(mono_box(&chat))).child(w::button("chat-save", "Save").on_click(|_, _, cx| chat_save(cx))))
                .child(hint("Where messages go. Send /start to the bot and it fills itself in, or enter it by hand."))
                .into_any_element())
        } else { None };
        let id = ch.id.clone();
        d = d.part(|| toggle_row(("chan", i), &ch.label, None, &ch.hint, ch.enabled, ch.can_toggle, extra, move |on, cx| channel_toggle(cx, &id, on)));
    }
    d
}

fn notify_body(want: Want, cx: &mut App) -> Parts {
    let s = st(cx);
    let cell = |i: usize, m: &'static str, on: bool, key: String| div().w(T::SETTINGS_NTF_COL).h_full().flex().items_center().justify_center().border_l_1().border_color(T::border())
        .child(div().id(("ntf", i * 3 + ["always", "away", "never"].iter().position(|x| *x == m).unwrap_or(0))).cursor_pointer().child(w::choice_mark(false, on, false))
            .on_click(move |_, _, cx| notify_pref(cx, &key, m)));
    let head = div().flex().items_stretch().min_h(T::CONTROL_LG).pl(T::SP_12).bg(T::bg_tertiary())
        .child(div().flex_1().min_w_0().flex().items_center().font_family(T::MONO).text_size(T::TEXT_10).text_color(T::text_muted()).child("EVENT"))
        .children(["ALWAYS", "WHEN AWAY", "NEVER"].into_iter().map(|m| div().w(T::SETTINGS_NTF_COL).flex().items_center().justify_center().border_l_1().border_color(T::border())
            .font_family(T::MONO).text_size(T::TEXT_10).text_color(T::text_muted()).child(m)));
    // The table's rows are parts of their own (Parts::framed_with), each with
    // the hairline over it, built only when wanted.
    let line = |i: usize, r: &NotifyRow| -> AnyElement {
        let line = if !r.cat.is_empty() {
            div().pl(T::SP_12).py(T::SP_6).bg(T::bg_tertiary()).text_size(T::TEXT_10).font_weight(T::W_BOLD).text_color(T::accent()).child(r.cat.clone()).into_any_element()
        } else {
            div().id(("ntf-row", i)).flex().items_stretch().min_h(T::CONTROL_LG).pl(T::SP_12).hover(|d| d.bg(T::bg_hover()))
                .child(div().flex_1().min_w_0().flex().items_center().text_size(T::TEXT_13).text_color(T::text()).child(r.label.clone()))
                .child(cell(i, "always", r.mode == "always", r.key.clone()))
                .child(cell(i, "away", r.mode == "away", r.key.clone()))
                .child(cell(i, "never", r.mode == "never", r.key.clone())).into_any_element()
        };
        div().flex().flex_col().child(hairline()).child(line).into_any_element()
    };
    let mut head = Some(head);
    parts(T::SP_12).wanting(want)
        .part(|| hint("Which events are worth a notification, sent on whichever channels are on above. \u{201c}Always\u{201d} sends every time; \u{201c}When Away\u{201d} only while you\u{2019}re not focused on Crundi (browser tab / desktop app); \u{201c}Never\u{201d} is off."))
        .framed_with(s.notify.len() + 1, T::R_CARD, T::border(), Some(T::bg_secondary()), |k| match k {
            0 => head.take().map(|h| h.into_any_element()).unwrap_or_else(|| div().into_any_element()),
            k => line(k - 1, &s.notify[k - 1]),
        })
}

fn oauth_block(window: &mut Window, cx: &mut App, k: &'static str) -> AnyElement {
    let (a, b, d) = if k == "google" { ("google-id", "google-secret", "google-allowed") } else { ("github-id", "github-secret", "github-allowed") };
    let (ph_sec, ph_al) = get(cx, |s| { let o = if k == "google" { &s.google } else { &s.github }; (if o.v.secret_set { "Saved (leave blank to keep)" } else { "Not set" }, o.v.ph.clone()) });
    let id = c::line::<St>(window, cx, a, "", false, None);
    let sec = c::line::<St>(window, cx, b, ph_sec, true, None);
    let al = c::line::<St>(window, cx, d, &ph_al, false, None);
    let s = st(cx);
    let o = if k == "google" { &s.google } else { &s.github };
    let url = o.v.url.clone();
    let open = if k == "google" { "https://console.cloud.google.com/apis/credentials" } else { "https://github.com/settings/applications/new" };
    col(T::SP_8).pt(T::SP_6)
        .child(hairline())
        .child(toggle_row(eid("oauth-on", k), &o.v.label, Some(if k == "google" { GOOGLE } else { GITHUB }), &o.v.status, o.v.on, o.v.can_on || o.v.on, None, move |on, cx| method_toggle(cx, k, on)))
        .child(label(format!("Callback URL to give {}", o.v.label)))
        .child(row().child(div().flex_1().min_w_0().px(T::SP_10).py(T::SP_8).bg(T::bg()).rounded(T::R_CONTROL).border_1().border_color(T::border())
                .font_family(T::MONO).text_size(T::TEXT_12).text_color(T::text())
                .child(if url.is_empty() { "Unknown until this server has a public address".to_string() } else { url.clone() }))
            .when(!url.is_empty(), |d| d.child(w::button(eid("oauth-copy", k), "Copy").on_click(move |_, _, cx| oauth_copy(cx, k)))))
        .child(hint_link(eid("oauth-open", k), &o.v.howto, "Open", if o.v.local { " This is a local address: it only works when signing in on this computer." } else { "" }, open))
        .child(div().flex().gap(T::SP_8)
            .child(col(T::SP_4).flex_1().min_w_0().child(label("Client ID")).child(mono_box(&id)))
            .child(col(T::SP_4).flex_1().min_w_0().child(label("Client secret")).child(mono_box(&sec))))
        .child(col(T::SP_4).child(label(o.v.acct.clone())).child(input_box(&al)))
        .child(row()
            .child(w::primary(eid("oauth-save", k), "Save").on_click(move |_, _, cx| oauth_save(cx, k)))
            .child(w::disabled(w::button(eid("oauth-test", k), "Test"), !o.v.configured).on_click(move |_, _, cx| oauth_test(cx, k)))
            .when(o.v.configured, |d| d.child(w::button(eid("oauth-clear", k), "Remove").on_click(move |_, _, cx| oauth_clear(cx, k))))
            .child(status(o.test_line.clone(), "")))
        .into_any_element()
}

fn signin_body(want: Want, window: &mut Window, cx: &mut App) -> Parts {
    let pw_set = get(cx, |s| s.pw_set);
    let tg_set = get(cx, |s| s.tg_token_set);
    let pw = c::line::<St>(window, cx, "new-password", if pw_set { "New password (12+ characters)" } else { "Choose a password (12+ characters)" }, true, Some(on_key));
    let tok = c::line::<St>(window, cx, "tg-token", if tg_set { "Saved (leave blank to keep)" } else { "123456:ABC-DEF..." }, true, None);
    let user = c::line::<St>(window, cx, "tg-user", "your-username", false, None);
    let oauth = if get(cx, |s| s.oauth_has) { vec![oauth_block(window, cx, "google"), oauth_block(window, cx, "github")] } else { vec![] };
    let s = st(cx);
    let mut d = parts(T::SP_10).wanting(want)
        .part(|| hint("Any one of these is enough, any mix can be on, and one must always stay on."))
        .part(|| toggle_row("pw-on", "Password + authenticator code", None,
            if s.pw_set { "A password and a 6-digit code. Setting a new password issues a new code source." } else { "Not set up yet. Choose a password below to enable it." },
            s.pw_on, s.pw_set, None, |on, cx| method_toggle(cx, "password", on)))
        .part(|| row().child(div().flex_1().min_w_0().child(w::input(&pw))).child(w::primary("pw-set", if s.pw_set { "Change" } else { "Set" }).on_click(|_, _, cx| set_password(cx))))
        .when(!s.enrol.is_empty(), |d| d.part(|| col(T::SP_6).p(T::SP_12).rounded(T::R_FIELD).border_1().border_color(T::accent()).bg(T::accent_dim())
            .child(div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Add this to your authenticator app"))
            .child(hint("Shown once. Without it you will not be able to sign in with the new password."))
            .child(div().p(T::SP_8).bg(T::bg()).rounded(T::R_CONTROL).border_1().border_color(T::border()).flex().justify_center()
                .font_family(T::MONO).text_size(T::TEXT_15).text_color(T::text()).child(s.enrol.clone()))));
    // Telegram (authTelegramSettings): an older server sends no bot block, so only the switch.
    if !s.tg_has {
        d = d.part(|| toggle_row("tg-on", "Telegram", Some(TELEGRAM), "", s.tg_on, s.tg_can || s.tg_on, None, |on, cx| method_toggle(cx, "telegram", on)));
    } else {
        d = d.part(|| col(T::SP_8).pt(T::SP_6).child(hairline())
            .child(toggle_row("tg-on", "Telegram", Some(TELEGRAM), &s.tg_status, s.tg_on, s.tg_can || s.tg_on, None, |on, cx| method_toggle(cx, "telegram", on)))
            .child(hint_link("tg-open", "In Telegram, message @BotFather, send /newbot and copy the token it gives you. The same bot sends the Telegram notification channel.", "Open", "", "https://t.me/BotFather"))
            .child(div().flex().gap(T::SP_8)
                .child(col(T::SP_4).flex_1().min_w_0().child(label("Bot token")).child(mono_box(&tok)))
                .child(col(T::SP_4).flex_1().min_w_0().child(label("Allowed Telegram username")).child(input_box(&user))))
            .child(row().child(w::primary("tg-save", "Save").on_click(|_, _, cx| tg_save(cx)))
                .when(s.tg_restart, |d| d.child(w::button("tg-restart", "Restart now").on_click(|_, _, cx| restart_server(cx))))
                .when(s.tg_token_set, |d| d.child(w::button("tg-clear", "Remove").on_click(|_, _, cx| tg_clear(cx))))));
    }
    d.children(oauth)
}

fn phone_body(want: Want, window: &mut Window, cx: &mut App) -> Parts {
    let has_key = get(cx, |s| s.push_has_key);
    let key = c::line::<St>(window, cx, "push-key", if has_key { "Paste a new key to replace it" } else { "Paste the push key" }, true, Some(on_key));
    let relay = c::line::<St>(window, cx, "relay-url", "https://your-relay.example", false, None);
    let s = st(cx);
    let mut d = parts(T::SP_10).wanting(want)
        .part(|| row().gap(T::SP_12).child(hint("Pair the Crundi app on a phone with this server. It shows a picture for the phone to scan, so no password is typed on the phone.").flex_1().min_w_0())
            .child(w::button("phone-connect", "Connect a phone").on_click(|_, _, cx| pair_start(cx))))
        .part(|| label("Notifications on the phone"));
    match s.push_state.as_str() {
        "checking" => d = d.part(|| hint("Checking\u{2026}").text_size(T::TEXT_12)),
        "none" => d = d.part(|| hint("Phone notifications are not available on this server.").text_size(T::TEXT_12)),
        _ => {
            let url = s.push_contact_url.clone();
            let custom = s.relay_custom;
            d = d.part(|| col(T::SP_8)
                .child(label(s.push_line.clone()))
                .when(!s.push_contact.is_empty(), |d| d.child(row().child(w::link("push-contact", s.push_contact.clone()).on_click(move |_, _, cx| open_url(cx, &url)))))
                .when(!s.push_error.is_empty(), |d| d.child(hint(s.push_error.clone()).text_color(T::yellow())))
                .child(row().child(div().flex_1().min_w_0().child(w::input(&key))).child(w::primary("push-save", "Save").on_click(|_, _, cx| push_save(cx)))
                    .when(s.push_has_key, |d| d.child(w::button("push-mtest", "Test").on_click(|_, _, cx| push_mobile_test(cx))).child(w::button("push-remove", "Remove").on_click(|_, _, cx| push_remove(cx)))))
                // A custom relay is for a custom build of the app: folded away (<details>).
                .child(div().id("relay-fold").flex().items_center().gap(T::SP_4).cursor_pointer().text_size(T::TEXT_12).text_color(T::text_muted())
                    .child(w::icon(if s.relay_open { IconName::ChevronDown } else { IconName::ChevronRight }, T::ICON_SM)).child(format!("Relay: {}", s.relay_summary))
                    .on_click(|_, _, cx| upd(cx, |s| s.relay_open = !s.relay_open)))
                .when(s.relay_open, |d| d.child(col(T::SP_8)
                    .child(hint("Only change this if you use your own build of the app, with its own relay. The published app works with the published relay and no other. Changing the relay removes the key."))
                    .child(row().child(div().flex_1().min_w_0().child(w::input(&relay)))
                        .child(w::button("relay-save", if custom { "Save" } else { "Use this relay" }).on_click(|_, _, cx| { let u = field(cx, "relay-url"); relay_save(cx, u); }))
                        .when(custom, |d| d.child(w::button("relay-pub", "Use the published relay").on_click(|_, _, cx| relay_save(cx, String::new()))))))));
        }
    }
    d
}

fn session_body(want: Want, cx: &mut App) -> Parts {
    let s = st(cx);
    let server = s.server.clone();
    let mut d = parts(T::SP_10).wanting(want)
        .part(|| row().gap(T::SP_12).child(hint("Ends this sign-in on this device and revokes its tokens, so they cannot be refreshed. Also clears the cookie that reaches private forwards.").flex_1().min_w_0())
            // Two-step, as the web's: easy to hit by accident, and the way back is the sign-in screen.
            .child(w::arm_button("sess-signout", format!("Sign out of {server}"), None, "Sign out?", |_, cx| crate::compat::session::sign_out(cx, ""))))
        .part(|| row().child(label("Signed in devices").flex_1().min_w_0()).child(w::button("sess-toggle", if s.sess_open { "Hide" } else { "Show" }).on_click(|_, _, cx| sess_toggle(cx))));
    if !s.sess_open { return d; }
    match s.sess_state.as_str() {
        "loading" => d = d.part(|| hint("Loading\u{2026}").text_size(T::TEXT_13)),
        "error" => d = d.part(|| hint(format!("Could not load sessions: {}", s.sess_error)).text_size(T::TEXT_13)),
        "empty" => d = d.part(|| hint("No active sessions.").text_size(T::TEXT_13)),
        _ => {
            let mut list = col(px(0.));
            for (i, r) in s.sessions.iter().enumerate() {
                if i > 0 { list = list.child(hairline()); }
                let id = r.id.clone();
                list = list.child(row().gap(T::SP_12).py(T::SP_8)
                    .child(col(T::SP_2).flex_1().min_w_0()
                        .child(row().gap(T::SP_6).child(div().text_size(T::TEXT_13).text_color(T::text()).truncate().child(r.label.clone()))
                            .when(r.current, |d| d.child(w::tag("this device", "accent"))))
                        .child(hint(r.desc.clone())))
                    .child(if r.current { div().text_size(T::TEXT_12).text_color(T::text_muted()).child("\u{2014}").into_any_element() }
                        else { w::arm_button(("sess-revoke", i), "Revoke", None, "Revoke?", move |_, cx| sess_revoke(cx, id.clone())).look("danger").into_any_element() }));
            }
            let n = s.sess_others;
            d = d.part(|| list).when(n > 0, |d| d.child(row().pt(T::SP_10).justify_end()
                .child(w::arm_button("sess-others", format!("Sign out the other {}", if n == 1 { "device".to_string() } else { format!("{n} devices") }), None, "Sign them all out?", |_, cx| sess_revoke_others(cx)).look("danger"))));
        }
    }
    d
}

fn config_field(e: &Entity<w::InputState>, label_text: &str, hint_text: &str) -> Div {
    col(T::SP_4).child(label(label_text.to_string())).child(mono_box(e)).child(hint(hint_text.to_string()))
}

fn config_body(want: Want, window: &mut Window, cx: &mut App) -> Parts {
    let pd = c::line::<St>(window, cx, "projects-dir", "C:\\Projects", false, None);
    let wp = c::line::<St>(window, cx, "web-port", "0 (auto)", false, None);
    let tt = c::line::<St>(window, cx, "tunnel-token", "(optional)", true, None);
    let tu = c::line::<St>(window, cx, "tunnel-url", "https://crundi.example.com", false, None);
    let dd = c::line::<St>(window, cx, "data-dir", "(default: platform app dir)", false, None);
    let s = st(cx);
    parts(T::SP_12).wanting(want)
        .part(|| row().gap(T::SP_6).child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Changes are saved to"))
            .child(div().px(T::SP_4).py(T::SP_2).rounded(T::R_CHIP).bg(T::bg_tertiary()).font_family(T::MONO).text_size(T::TEXT_11).text_color(T::text_secondary()).truncate().child(s.env_path.clone())))
        .part(|| config_field(&pd, "Projects Directory", "Single mode: subfolders auto-discovered. Leave empty for multi mode (add projects individually)."))
        .part(|| config_field(&wp, "Web Port", "Port for the web UI. 0 = auto-assign."))
        .part(|| config_field(&tt, "Cloudflare Tunnel Token", "For a persistent custom domain. Leave empty for random quick tunnel."))
        .part(|| config_field(&tu, "Cloudflare Tunnel URL", "Your public tunnel URL (used with named tunnel token above)."))
        .part(|| config_field(&dd, "Data Directory", "Override data storage location. Leave empty for default."))
        .part(|| row().child(w::primary("config-save", "Save").on_click(|_, _, cx| save_config(cx))).child(status(s.config_status.clone(), &s.config_tone)))
}

fn terminal_body(want: Want, window: &mut Window, cx: &mut App) -> Parts {
    let ap = c::line::<St>(window, cx, "autopark", "", false, Some(on_key));
    let s = st(cx);
    parts(T::SP_12).wanting(want)
        .part(|| col(T::SP_4).child(label("New Line Key"))
            .child(row().child(w::segmented("newline-key", &["Shift + Enter", "Ctrl + Enter"], s.newline_key.max(0) as usize, |i, _, cx| set_newline(cx, *i as i32))))
            .child(hint("Inserts a newline in the terminal instead of submitting \u{2014} the other key still submits. Applies instantly to all terminals.")))
        .part(|| col(T::SP_4).child(label("Close idle panes after (minutes)"))
            .child(div().w(T::SETTINGS_NUM_W).child(w::input(&ap)))
            .child(hint(format!("A chat or Claude terminal with nothing going on closes once you have also been away this long. It keeps its place, shows what it was, and Resume picks the conversation back up. 0 turns it off; otherwise at least {}. Shells are never closed for being idle. After a restart, every pane that was open comes back the same way.", s.autopark_min))))
}

fn desktop_body(want: Want, cx: &mut App) -> Parts {
    let s = st(cx);
    parts(T::SP_12).wanting(want)
        .part(|| info_row("Launch at startup", &s.launch_hint, vec![w::switch("launch", s.launch, |v, _, cx| launch_toggle(cx, *v)).into_any_element()]))
        .part(|| info_row("Automatic updates", "Check on launch and once a day. Always asks before installing.", vec![w::switch("auto-update", s.auto_update, |v, _, cx| update_toggle(cx, *v)).into_any_element()]))
        .part(|| info_row("Release channel", "Dev gets preview builds early. A newer production release is offered either way.",
            vec![w::segmented("upd-channel", &["Production", "Dev"], s.upd_channel.max(0) as usize, |i, _, cx| desktop_channel(cx, *i as i32)).into_any_element()]))
        .part(|| row().gap(T::SP_12)
            .child(row().flex_1().min_w_0().child(w::dot(if s.upd_avail { T::accent() } else { T::green() }))
                .child(div().text_size(T::TEXT_13).text_color(if s.upd_avail { T::accent_hover() } else { T::text_secondary() }).child(s.upd_status.clone())))
            .child(if s.upd_avail {
                w::disabled(w::primary("update-install", if s.upd_downloaded { "Restart & install" } else if s.upd_downloading { "Downloading\u{2026}" } else { "Download & install" }), s.upd_downloading)
                    .on_click(|_, _, cx| update_install(cx))
            } else { w::button("update-check", "Check now").on_click(|_, _, cx| update_check(cx)) }))
}

/// Connect a phone (crundi-phone.js connect()): the picture, the line under
/// it, the countdown, the swap and copy links, New code and Close.
fn pair_dialog(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    if !get(cx, |s| s.pair_open) { return None; }
    let focus = match get(cx, |s| s.pair_focus.clone()) {
        Some(f) => f,
        None => { let f = cx.focus_handle(); upd(cx, |s| s.pair_focus = Some(f.clone())); f }
    };
    let s = st(cx);
    let pic = div().size(T::SETTINGS_PAIR_PIC).rounded(T::R_BUBBLE + T::SP_10).bg(T::backdrop()).overflow_hidden().flex().items_center().justify_center()
        .child(match &s.pair_image {
            Some(i) => img(i.clone()).size_full().object_fit(ObjectFit::Contain).into_any_element(),
            None => div().text_size(T::TEXT_13).text_color(T::text_muted()).child(s.pair_pic_text.clone()).into_any_element(),
        });
    let body = col(T::SP_10).p(T::SP_10).items_center()
        .child(pic)
        .child(div().text_size(T::TEXT_13).text_color(if s.pair_done { T::green() } else { T::text_secondary() }).font_weight(if s.pair_done { T::W_BOLD } else { T::W_REGULAR }).child(s.pair_line.clone()))
        .when(!s.pair_small.is_empty(), |d| d.child(hint(s.pair_small.clone())))
        .when(!s.pair_swap.is_empty(), |d| d.child(w::link("pair-swap", s.pair_swap.clone()).on_click(|_, _, cx| pair_swap(cx))))
        .when(!s.pair_copy.is_empty(), |d| d.child(w::link("pair-copy", s.pair_copy.clone()).on_click(|_, _, cx| pair_copy(cx))))
        .child(row().gap(T::SP_10)
            .when(s.pair_again, |d| d.child(w::primary("pair-again", "New code").on_click(|_, _, cx| pair_begin(cx))))
            .child(w::button("pair-close", "Close").on_click(|_, _, cx| pair_close(cx))));
    let m = w::Modal {
        id: "pair-dialog", width: T::DIALOG_W_LG, focus, scroll: s.pair_scroll.clone(),
        on_close: std::rc::Rc::new(|_, cx| pair_close(cx)),
        on_key: std::rc::Rc::new(|e: &KeyDownEvent, _, cx| if e.keystroke.key == "escape" { pair_close(cx); }),
    };
    let children = vec![w::modal_head("pair-x", "Connect a phone", false, |_, _, cx| pair_close(cx)).into_any_element(), body.into_any_element()];
    Some(w::overlay_modal(m, window, children).into_any_element())
}

// ─── The test driver ───

/// The driver's "ops:settings-*" actions: they call what the controls call.
pub fn test_ui(name: &str, args: &[String], _window: &mut Window, cx: &mut App) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    if !name.starts_with("settings-") { return None; }
    ensure(cx);
    match name {
        "settings-reload" => render(cx),
        // Bring the open skill into view (the snapshot of an open skill).
        "settings-reveal-skill" => upd(cx, |s| s.reveal_skill = true),
        // Open or close a section by its key (server, cc, signin…).
        "settings-open" => if !set_open(cx, &a(0), a(1) != "false") { return Some(json!({ "ok": false, "error": "no such section" })); },
        // Type into a field, as a person would.
        "settings-field" => match FIELDS.iter().find(|k| **k == a(0)) {
            Some(k) => set_field(cx, k, a(1)),
            None => return Some(json!({ "ok": false, "error": "no such field" })),
        },
        // Press a control: its name and its arguments.
        "settings-do" => match a(0).as_str() {
            "save-config" => save_config(cx),
            "save-prompts" => save_prompts(cx),
            "srv-check" => server_update(cx, true),
            "srv-set-channel" => server_channel(cx, a(1).parse().unwrap_or(0)),
            "srv-apply" => server_apply(cx),
            "cc-check" => claude_update(cx, true),
            "cc-apply" => claude_apply(cx),
            "restart" => restart_server(cx),
            "whats-new" => whats_new(cx),
            "notify-pref" => notify_pref(cx, &a(1), &a(2)),
            "channel-toggle" => channel_toggle(cx, &a(1), a(2) == "true"),
            "chat-save" => chat_save(cx),
            "push-test" => push_test(cx),
            "goto-signin-telegram" => { set_open(cx, "signin", true); }
            "set-password" => set_password(cx),
            "method-toggle" => method_toggle(cx, &a(1), a(2) == "true"),
            "tg-save" => tg_save(cx),
            "tg-clear" => tg_clear(cx),
            "oauth-save" => oauth_save(cx, &a(1)),
            "oauth-test" => oauth_test(cx, &a(1)),
            "oauth-clear" => oauth_clear(cx, &a(1)),
            "oauth-copy" => oauth_copy(cx, &a(1)),
            "phone-connect" => pair_start(cx),
            "pair-close" => pair_close(cx),
            "pair-again" => pair_begin(cx),
            "pair-swap" => pair_swap(cx),
            "push-save" => push_save(cx),
            "push-mobile-test" => push_mobile_test(cx),
            "push-remove" => push_remove(cx),
            "relay-save" => relay_save(cx, a(1)),
            "sess-toggle" => sess_toggle(cx),
            "sess-revoke" => sess_revoke(cx, a(1)),
            "sess-revoke-others" => sess_revoke_others(cx),
            "set-newline-key" => set_newline(cx, a(1).parse().unwrap_or(0)),
            "autopark-commit" => { let t = field(cx, "autopark"); autopark_commit(cx, &t); }
            "vs-pick" => voice_pick(cx, &a(1), a(2).parse().unwrap_or(0)),
            "vs-save-spend" => voice_save_spend(cx),
            "vs-save-keys" => voice_save_keys(cx),
            "vs-set-admin" => voice_admin(cx, a(1) == "true"),
            "vs-set-away" => voice_away(cx, Some(a(1) == "true"), None),
            "vs-reset" => voice_reset(cx),
            "launch-toggle" => launch_toggle(cx, a(1) == "true"),
            "update-toggle" => update_toggle(cx, a(1) == "true"),
            "upd-set-channel" => desktop_channel(cx, a(1).parse().unwrap_or(0)),
            "update-check" => update_check(cx),
            "update-install" => update_install(cx),
            _ => return Some(json!({ "ok": false, "error": "no such control" })),
        },
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// This screen's part of the area's props (under "settings"): the Slint
/// client's names.
pub fn props(_window: &mut Window, cx: &mut App) -> Value {
    ensure(cx);
    let s = st(cx);
    let oauth = |o: &OAuth| json!({ "on": o.v.on, "canOn": o.v.can_on, "status": o.v.status, "configured": o.v.configured, "url": o.v.url, "testLine": o.test_line });
    let f = |k: &str| s.f.get(k);
    json!({
        "state": s.state, "error": s.error,
        "open": SECTIONS.iter().map(|k| (k.to_string(), json!(s.open.get(k).copied().unwrap_or(false)))).collect::<Map<_, _>>(),
        "server": { "body": s.srv_body, "version": s.srv.version, "sub": s.srv.sub, "act": s.srv.act, "channel": s.srv.channel, "canRestart": s.srv.can_restart },
        "claude": { "body": s.cc_body, "version": s.cc.version, "sub": s.cc.sub, "act": s.cc.act, "beyond": s.cc.beyond },
        "config": { "envPath": s.env_path, "projectsDir": f("projects-dir"), "status": s.config_status },
        "prompt": { "all": f("sysprompt"), "project": s.proj_name, "projectText": f("proj-sysprompt"), "status": s.sp_status },
        "notify": s.notify.iter().filter(|r| r.cat.is_empty()).map(|r| (r.key.clone(), json!(r.mode))).collect::<Map<_, _>>(),
        "channels": s.channels.iter().map(|ch| json!({ "id": ch.id, "enabled": ch.enabled, "canToggle": ch.can_toggle, "telegram": ch.telegram })).collect::<Vec<_>>(),
        "telegramChannel": { "state": s.tgc_state, "chatId": f("chat-id") },
        "signin": {
            "pwSet": s.pw_set, "pwOn": s.pw_on, "enrol": s.enrol,
            "tgHas": s.tg_has, "tgOn": s.tg_on, "tgStatus": s.tg_status, "tgTokenSet": s.tg_token_set, "tgUser": f("tg-user"),
            "oauth": s.oauth_has, "google": oauth(&s.google), "github": oauth(&s.github), "googleId": f("google-id"),
        },
        "phone": { "push": s.push_state, "line": s.push_line, "contact": s.push_contact,
            "pairOpen": s.pair_open, "pairLine": s.pair_line, "pairAgain": s.pair_again, "pairImage": s.pair_image.is_some() },
        "session": { "open": s.sess_open, "state": s.sess_state, "others": s.sess_others,
            "rows": s.sessions.iter().map(|r| json!({ "id": r.id, "label": r.label, "current": r.current, "desc": r.desc })).collect::<Vec<_>>() },
        "terminal": { "newlineKey": s.newline_key, "autopark": f("autopark"), "min": s.autopark_min },
        "voice": { "shown": s.voice.shown, "model": s.voice.sel.get("model").map(|e| e.1).unwrap_or(-1), "models": s.voice.sel.get("model").map(|e| e.0.len()).unwrap_or(0),
            "status": s.voice.status, "admin": s.voice.admin, "spend": s.voice.spend, "spendNote": s.voice.spend_note, "keysNote": s.voice.keys_note,
            "away": s.voice.away, "awayClasses": s.voice.away_classes.len() },
        "desktop": { "launch": s.launch, "autoUpdate": s.auto_update, "channel": s.upd_channel, "status": s.upd_status, "dot": s.upd_avail, "downloading": s.upd_downloading, "downloaded": s.upd_downloaded },
        "modal": s.pair_open,
        "opened": c::s(&s.data, "opened"),
        "ptt": crate::ptt::label(&crate::ptt::get()), "pttCapturing": s.ptt_capturing,
    })
}

#[cfg(test)]
mod gpui_tests {
    use super::{ent, is_open, quiet, set_open};
    use std::{cell::Cell, rc::Rc};

    #[gpui::test]
    fn the_state_notifies_its_page_and_quiet_changes_do_not(cx: &mut gpui::TestAppContext) {
        let n = Rc::new(Cell::new(0));
        let n2 = n.clone();
        let _sub = cx.update(|cx| { let e = ent(cx); cx.observe(&e, move |_, _| n2.set(n2.get() + 1)) });
        cx.update(|cx| {
            assert!(set_open(cx, "usage", true));
            assert!(is_open(cx, "usage"));
            assert!(!set_open(cx, "nothing-such", true));
            quiet(cx, |s| s.scroll_to = None);
        });
        cx.run_until_parked();
        assert_eq!(n.get(), 1, "one section opened: one notify; a quiet change none");
    }
}
