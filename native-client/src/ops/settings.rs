//! Ops area: Settings, for the owner (the web's renderSettings,
//! organizeSettings, saveSettings, saveSystemPrompts, saveNotifyPrefs,
//! authTelegramSettings, authOAuthSettings/authOAuthTest, the update
//! sections, the phone app, the session, Cru's server settings), and the PC
//! client's own "Desktop App" section. Skills, Backup and Usage are
//! src/ops/skills.rs's.
//! UI: ui/ops/settings.slint (global OpsSettings). Pure rules: settings/rules.rs.

use crate::{areas::AreaEvent, kit, MainWindow, OpsSettings};
use serde_json::{json, Map, Value};
use slint::{ComponentHandle, Model, SharedString, Timer, TimerMode};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::{Duration, Instant}};

#[path = "settings/rules.rs"]
pub mod rules;
#[path = "settings/local.rs"]
mod local;

use rules::*;

// ─── State ───

#[derive(Default)]
struct Voice {
    /// Each select's values, in the order of its options.
    values: HashMap<&'static str, Vec<Value>>,
    away_allow: Vec<String>,
}

struct Pair {
    code: String,
    link: String,
    qr: String,
    mark: String,
    showing_qr: bool,
    note: String,
    warn: String,
    left: i64,
    paired: bool,
    tick: Timer,
    poll: Timer,
}

struct OAuthPoll { timer: Timer, poll_id: String, until: Instant, busy: bool }

#[derive(Default)]
struct St {
    shown: bool,
    loaded: bool,
    data: Value,
    prefs: Map<String, Value>,
    sp_max: usize,
    autopark: i64,
    srv_checked: Option<Instant>,
    srv_poll: Option<Timer>,
    oauth_polls: HashMap<String, OAuthPoll>,
    voice: Voice,
    pair: Option<Pair>,
    /// The project prompts saved here, until the projects list catches up.
    proj_prompts: HashMap<String, String>,
    whats_new: Option<Rc<dyn Fn()>>,
}

thread_local! {
    static ST: RefCell<St> = RefCell::new(St { sp_max: 8000, autopark: 60, ..Default::default() });
}

fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { ST.with(|s| f(&mut s.borrow_mut())) }

fn ui(f: impl FnOnce(&OpsSettings)) {
    if let Some(w) = kit::ui() { f(&w.global::<OpsSettings>()); }
}

fn now_secs() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// A server answer with `ok: false` is an error (its words, or `fallback`).
fn ok(r: Result<Value, String>, fallback: &str) -> Result<Value, String> {
    match r {
        Ok(v) if v.get("ok") == Some(&Value::Bool(false)) => {
            let e = kit::s(&v, "error");
            Err(if e.is_empty() { fallback.to_string() } else { e })
        }
        Ok(v) => Ok(v),
        Err(e) => Err(if e.is_empty() { fallback.to_string() } else { e }),
    }
}

fn post(path: &'static str, body: Value, done: impl FnOnce(Result<Value, String>) + Send + 'static) {
    kit::call(move |api| api.post(path, &body), done);
}

/// Run `f` once after `ms` (kept alive in the state).
fn after(ms: u64, f: impl FnOnce() + 'static) {
    Timer::single_shot(Duration::from_millis(ms), f);
}

fn sv(v: &[String]) -> slint::ModelRc<SharedString> {
    slint::ModelRc::new(slint::VecModel::from(v.iter().map(SharedString::from).collect::<Vec<_>>()))
}

/// What "Show it again" opens (the What's new tour is src/ops/extras.rs's):
/// it hands its opener here.
pub fn set_whats_new(f: impl Fn() + 'static) { with(|s| s.whats_new = Some(Rc::new(f))); }

/// The PC client's own update (the ship phase's updater): the tab's dot and
/// the Desktop App section's status line.
pub fn set_update_available(available: bool, label: &str) {
    if let Some(w) = kit::ui() {
        w.global::<crate::Shell>().set_update_dot(available);
        let g = w.global::<OpsSettings>();
        g.set_upd_avail(available);
        if !label.is_empty() { g.set_upd_status(label.into()); }
    }
}

/// The terminal's newline key ("shift" or "ctrl"), for the terminal panels.
pub fn newline_key() -> String { local::load().newline_key() }

// ─── The registry ───

pub fn init(w: &MainWindow) {
    let g = w.global::<OpsSettings>();
    g.on_reload(|| render());
    g.on_open_url(|u| open_url(&u));
    g.on_restart(restart_server);
    // Server and Claude Code.
    g.on_srv_check(|| server_update(true));
    g.on_srv_apply(server_apply);
    g.on_srv_set_channel(server_channel);
    g.on_cc_check(|| claude_update(true));
    g.on_cc_apply(claude_apply);
    g.on_whats_new(|| {
        let f = with(|s| s.whats_new.clone());
        // The web's openWhatsNew(true): the whole tour, or "Nothing new to show for this version."
        match f { Some(f) => f(), None => crate::ops::extras::open_whatsnew(true) }
    });
    // Configuration, prompts, terminal.
    g.on_save_config(save_config);
    g.on_save_prompts(save_prompts);
    g.on_set_newline_key(|i| {
        local::update(|l| l.newline = if i == 1 { "ctrl".into() } else { "shift".into() });
        ui(|g| g.set_newline_key(i));
    });
    g.on_autopark_commit(|t| autopark_commit(&t));
    // Notifications.
    g.on_channel_toggle(|id, on| channel_toggle(&id, on));
    g.on_push_test(|| post("/api/push/test", json!({}), |r| match ok(r, "Could not send") {
        Ok(_) => kit::toast_ok("Test sent"),
        Err(e) => kit::toast_error(&e),
    }));
    g.on_chat_save(chat_save);
    g.on_goto_signin_telegram(|| ui(|g| g.set_open_signin(true)));
    g.on_notify_pref(|k, m| notify_pref(&k, &m));
    // Sign-in.
    g.on_set_password(set_password);
    g.on_method_toggle(|m, on| method_toggle(&m, on));
    g.on_tg_save(tg_save);
    g.on_tg_clear(tg_clear);
    g.on_oauth_save(|k| oauth_save(&k));
    g.on_oauth_test(|k| oauth_test(&k));
    g.on_oauth_clear(|k| oauth_clear(&k));
    g.on_oauth_copy(|k| {
        let url = with(|s| s.data.pointer(&format!("/auth/callbackUrls/{k}")).and_then(|u| u.as_str()).unwrap_or("").to_string());
        if copy(&url) { kit::toast_ok("Copied") } else { kit::toast_error("Select it and copy it by hand") }
    });
    // Phone app.
    g.on_phone_connect(pair_start);
    g.on_push_save(push_save);
    g.on_push_mobile_test(push_mobile_test);
    g.on_push_remove(|| kit::call(|api| api.delete("/api/push/mobile/key"), |r| match r {
        Ok(_) => { kit::toast("Push key removed"); push_load(); }
        Err(_) => kit::toast_error("Could not remove the key"),
    }));
    g.on_relay_save(|u| relay_save(u.to_string()));
    g.on_pair_close(pair_close);
    g.on_pair_swap_clicked(|| { with(|s| if let Some(p) = s.pair.as_mut() { p.showing_qr = !p.showing_qr; }); pair_show(); });
    g.on_pair_copy_clicked(pair_copy);
    g.on_pair_again_clicked(pair_begin);
    // Session.
    g.on_sess_toggle(|| {
        let open = kit::ui().map(|w| !w.global::<OpsSettings>().get_sess_open()).unwrap_or(false);
        ui(|g| g.set_sess_open(open));
        if open { sessions_load(); }
    });
    g.on_sess_revoke(|id| sess_revoke(id.to_string()));
    g.on_sess_revoke_others(sess_revoke_others);
    // Cru.
    g.on_vs_pick(|f, i| voice_pick(&f, i));
    g.on_vs_set_admin(voice_admin);
    g.on_vs_set_away(|on| voice_away(Some(on), None));
    g.on_vs_away_class(|id, on| voice_away(None, Some((id.to_string(), on))));
    g.on_vs_away_stop(voice_away_stop);
    g.on_vs_save_spend(voice_save_spend);
    g.on_vs_save_keys(voice_save_keys);
    g.on_vs_reset(|| post("/api/voice/reset", json!({}), |r| {
        let good = matches!(&r, Ok(v) if v.get("ok") != Some(&Value::Bool(false)));
        ui(|g| { g.set_vs_reset_note(if good { "Cru starts a fresh conversation" } else { "Could not reset" }.into()); g.set_vs_reset_bad(!good); });
    }));
    // The Desktop App.
    g.on_launch_toggle(|on| match local::set_launch(on) {
        Ok(()) => ui(|g| g.set_launch(local::launch())),
        Err(e) => kit::toast_error(&e),
    });
    g.on_update_toggle(|on| { local::update(|l| l.auto_update = Some(on)); ui(|g| g.set_auto_update(on)); });
    g.on_upd_set_channel(desktop_channel);
    g.on_update_check(|| {
        kit::toast("Checking for updates\u{2026}");
        // The ship phase brings the updater; until then this build has none to ask.
        after(600, || kit::toast("This build cannot update itself yet: install the newer one by hand"));
    });
    desktop_show(&g);
}

pub fn signed_in() {}

pub fn signed_out() {
    let polls = with(|s| {
        s.loaded = false;
        s.data = Value::Null;
        s.proj_prompts.clear();
        if let Some(t) = s.srv_poll.take() { t.stop(); }
        std::mem::take(&mut s.oauth_polls)
    });
    for p in polls.values() { p.timer.stop(); }
    pair_stop();
    ui(|g| {
        g.set_state("loading".into());
        g.set_pair_open(false);
        g.set_modal(false);
        g.set_enrol_secret("".into());
        g.set_sess_open(false);
    });
}

pub fn project(_alias: &str) {
    if with(|s| s.loaded) { show_project(); }
}

pub fn tab_shown(tab: &str) {
    let on = tab == "settings";
    with(|s| s.shown = on);
    if on { render(); }
}

pub fn event(e: &AreaEvent) {
    // Back online after a restart: what the page says may be stale.
    if let AreaEvent::Connected(true) = e {
        if with(|s| s.shown && s.loaded) { server_update(false); }
    }
}

// ─── Rendering ───

/// The web's renderSettings: the settings, how you sign in and where
/// notifications go; then the sections that load on their own.
fn render() {
    if kit::api().is_none() { return; }
    if !with(|s| s.loaded) { ui(|g| g.set_state("loading".into())); }
    kit::call(|api| {
        let s = api.get("/api/settings")?;
        let a = api.get("/api/auth/config").unwrap_or(Value::Null);
        let c = api.get("/api/notify/channels").unwrap_or(Value::Null);
        Ok(json!({ "s": s, "a": a, "c": c }))
    }, |r| {
        let r = r.and_then(|all| ok(Ok(all["s"].clone()), "Failed").map(|_| all));
        match r {
            Ok(all) => render_done(all),
            Err(e) => ui(|g| { g.set_error(e.into()); g.set_state("error".into()); }),
        }
    });
}

pub(crate) fn render_done(all: Value) {
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
    with(|s| {
        s.loaded = true;
        s.data = json!({ "settings": data.clone(), "auth": auth.clone() });
        s.prefs = prefs.clone();
        s.sp_max = sp_max;
        s.autopark = autopark;
    });
    let Some(w) = kit::ui() else { return };
    let g = w.global::<OpsSettings>();
    let set = data.get("settings").cloned().unwrap_or(Value::Null);
    g.set_env_path(kit::s(&data, "envPath").into());
    g.set_projects_dir(kit::s(&set, "PROJECTS_DIR").into());
    g.set_web_port(kit::s(&set, "WEB_PORT").into());
    g.set_tunnel_token(kit::s(&set, "CLOUDFLARE_TUNNEL_TOKEN").into());
    g.set_tunnel_url(kit::s(&set, "CLOUDFLARE_TUNNEL_URL").into());
    g.set_data_dir(kit::s(&set, "DATA_DIR").into());
    g.set_config_status("".into());
    g.set_config_tone("".into());
    g.set_sysprompt(kit::s(&data, "systemPrompt").into());
    g.set_sp_status("".into());
    g.set_sp_tone("".into());
    g.set_autopark(autopark.to_string().into());
    g.set_autopark_min(autopark_min as i32);
    g.set_newline_key(if local::load().newline_key() == "ctrl" { 1 } else { 0 });
    // Notifications.
    show_notify(&g, &prefs);
    show_channels(&g, &chans);
    // Sign-in.
    show_auth(&g, &auth);
    g.set_state("data".into());
    show_project();
    // The sections that load by themselves.
    server_update(false);
    claude_update(false);
    push_load();
    voice_load();
    if g.get_sess_open() { sessions_load(); }
    desktop_show(&g);
}

fn show_project() {
    let alias = kit::project();
    let p = kit::projects().into_iter().find(|p| kit::s(p, "alias") == alias && !alias.is_empty());
    let over = with(|s| s.proj_prompts.get(&alias).cloned());
    ui(|g| match &p {
        Some(p) => {
            let name = kit::s(p, "name");
            g.set_proj_name(if name.is_empty() { alias.clone().into() } else { name.into() });
            g.set_proj_sysprompt(over.unwrap_or_else(|| kit::s(p, "systemPrompt")).into());
        }
        None => { g.set_proj_name("".into()); g.set_proj_sysprompt("".into()); }
    });
}

fn show_notify(g: &OpsSettings, prefs: &Map<String, Value>) {
    let rows: Vec<crate::OpsSettingsNotify> = notify_rows(prefs).into_iter().map(|r| crate::OpsSettingsNotify {
        cat: r.cat.into(), key: r.key.into(), label: r.label.into(), mode: r.mode.into(),
    }).collect();
    let m = g.get_notify();
    if m.row_count() == rows.len() {
        for (i, r) in rows.into_iter().enumerate() { if m.row_data(i).as_ref() != Some(&r) { m.set_row_data(i, r); } }
    } else {
        g.set_notify(slint::ModelRc::new(slint::VecModel::from(rows)));
    }
}

fn show_channels(g: &OpsSettings, chans: &[Value]) {
    let mut rows = Vec::new();
    for c in chans {
        let available = kit::truthy(c.get("available").unwrap_or(&Value::Null));
        let always = kit::truthy(c.get("alwaysOn").unwrap_or(&Value::Null));
        let hint = if available { kit::s(c, "describe") } else {
            let u = kit::s(c, "unavailableReason");
            if u.is_empty() { kit::s(c, "describe") } else { u }
        };
        let id = kit::s(c, "id");
        let tg = id == "telegram" && c.get("telegram").map(|t| t.is_object()).unwrap_or(false);
        if tg {
            let t = &c["telegram"];
            let (tone, words) = telegram_channel_state(t);
            g.set_tgc_tone(tone.into());
            g.set_tgc_state(words.into());
            g.set_tgc_token_set(kit::truthy(t.get("tokenSet").unwrap_or(&Value::Null)));
            g.set_chat_id(kit::s(t, "chatId").into());
        }
        rows.push(crate::OpsSettingsChannel {
            id: id.into(), label: kit::s(c, "label").into(), hint: hint.into(),
            enabled: kit::truthy(c.get("enabled").unwrap_or(&Value::Null)),
            can_toggle: !always && available, always_on: always, available, telegram: tg,
        });
    }
    g.set_channels(slint::ModelRc::new(slint::VecModel::from(rows)));
}

fn show_auth(g: &OpsSettings, auth: &Value) {
    let t = |k: &str| kit::truthy(auth.get(k).unwrap_or(&Value::Null));
    g.set_pw_set(t("passwordSet"));
    g.set_pw_on(t("password"));
    g.set_new_password("".into());
    g.set_tg_on(t("telegram"));
    match auth.get("telegramBot").filter(|b| b.is_object()) {
        Some(tb) => {
            g.set_tg_has(true);
            g.set_tg_can(t("telegramAvailable"));
            g.set_tg_status(telegram_status(tb).into());
            g.set_tg_token_set(kit::truthy(tb.get("tokenSet").unwrap_or(&Value::Null)));
            g.set_tg_restart(kit::truthy(tb.get("restartRequired").unwrap_or(&Value::Null)));
            g.set_tg_token("".into());
            g.set_tg_user(kit::s(tb, "allowedUsername").into());
        }
        None => { g.set_tg_has(false); g.set_tg_can(t("telegramAvailable")); }
    }
    let has = auth.get("oauth").map(|o| o.is_object()).unwrap_or(false);
    g.set_oauth_has(has);
    if has {
        for k in ["google", "github"] {
            let v = oauth_view(auth, k);
            let line = if k == "google" { g.get_google().test_line } else { g.get_github().test_line };
            let o = crate::OpsSettingsOAuth {
                key: v.key.into(), label: v.label.into(), on: v.on, can_on: v.can_on, status: v.status.into(),
                url: v.url.into(), local: v.local, howto: v.howto.into(), acct: v.acct.into(), ph: v.ph.into(),
                configured: v.configured, secret_set: v.secret_set, test_line: line,
            };
            if k == "google" {
                g.set_google(o);
                g.set_google_id(v.client_id.into()); g.set_google_secret("".into()); g.set_google_allowed(v.allowed.into());
            } else {
                g.set_github(o);
                g.set_github_id(v.client_id.into()); g.set_github_secret("".into()); g.set_github_allowed(v.allowed.into());
            }
        }
    }
}

// ─── Configuration, prompts, terminal ───

fn save_config() {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<OpsSettings>();
    let settings = json!({
        "PROJECTS_DIR": g.get_projects_dir().as_str(),
        "WEB_PORT": g.get_web_port().as_str(),
        "CLOUDFLARE_TUNNEL_TOKEN": g.get_tunnel_token().as_str(),
        "CLOUDFLARE_TUNNEL_URL": g.get_tunnel_url().as_str(),
        "DATA_DIR": g.get_data_dir().as_str(),
    });
    g.set_config_status("Saving...".into());
    g.set_config_tone("".into());
    post("/api/settings", json!({ "settings": settings }), |r| match ok(r, "unknown") {
        Ok(_) => {
            kit::toast("Settings saved. Restart required for changes to take effect.");
            ui(|g| { g.set_config_status("Saved \u{2014} restart required".into()); g.set_config_tone("warn".into()); });
        }
        Err(e) => {
            kit::toast_error(&e);
            ui(|g| { g.set_config_status(format!("Error: {e}").into()); g.set_config_tone("".into()); });
        }
    });
}

fn clip(s: &str, max: usize) -> String { s.chars().take(max).collect() }

fn save_prompts() {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<OpsSettings>();
    let max = with(|s| s.sp_max);
    let all = clip(g.get_sysprompt().as_str(), max);
    let alias = kit::project();
    let proj = if g.get_proj_name().is_empty() || alias.is_empty() { None } else { Some(clip(g.get_proj_sysprompt().as_str(), max)) };
    g.set_sp_status("Saving...".into());
    g.set_sp_tone("".into());
    kit::call(move |api| {
        let r = api.post("/api/settings", &json!({ "systemPrompt": all }))?;
        if r.get("ok") == Some(&Value::Bool(false)) { return Ok(json!({ "ok": false, "error": kit::s(&r, "error"), "which": "all" })); }
        if let Some(text) = proj {
            let p = api.post("/api/projects/system-prompt", &json!({ "project": alias, "text": text }))?;
            if p.get("ok") == Some(&Value::Bool(false)) { return Ok(json!({ "ok": false, "error": kit::s(&p, "error"), "which": "project" })); }
            return Ok(json!({ "ok": true, "project": alias, "text": text }));
        }
        Ok(json!({ "ok": true }))
    }, |r| {
        let err = |e: String| { kit::toast_error(&e); ui(|g| { g.set_sp_status("Error".into()); g.set_sp_tone("error".into()); }); };
        match r {
            Ok(v) if v.get("ok") == Some(&Value::Bool(false)) => {
                let e = kit::s(&v, "error");
                err(if !e.is_empty() { e } else if kit::s(&v, "which") == "project" { "Could not save the project prompt".into() } else { "Failed to save".into() });
            }
            Ok(v) => {
                // Keep the shown text in step until the projects list catches up.
                if let Some(p) = v.get("project").and_then(|p| p.as_str()) { let t = kit::s(&v, "text"); with(|s| s.proj_prompts.insert(p.to_string(), t)); }
                kit::toast_ok("System prompt saved");
                ui(|g| { g.set_sp_status("Saved \u{2014} applies to new conversations".into()); g.set_sp_tone("".into()); });
            }
            Err(e) => err(format!("Failed to save: {e}")),
        }
    });
}

fn autopark_commit(text: &str) {
    let (cur, min) = with(|s| (s.autopark, 0));
    let min = kit::ui().map(|w| w.global::<OpsSettings>().get_autopark_min() as i64).unwrap_or(10).max(min);
    let n = match rules::autopark(text, min) {
        Ok(n) => n,
        Err(e) => { kit::toast_error(&e); ui(|g| g.set_autopark(cur.to_string().into())); return; }
    };
    if n == cur { return; }
    post("/api/settings", json!({ "autoParkMinutes": n }), move |r| match ok(r, "Could not save") {
        Ok(_) => {
            with(|s| s.autopark = n);
            ui(|g| g.set_autopark(n.to_string().into()));
            kit::toast_ok(&if n > 0 { format!("Idle panes close after {n} minutes") } else { "Idle panes stay open".into() });
        }
        Err(e) => { kit::toast_error(&e); ui(|g| g.set_autopark(cur.to_string().into())); }
    });
}

// ─── Notifications ───

fn channel_toggle(id: &str, on: bool) {
    let id2 = id.to_string();
    post("/api/notify/channels", json!({ "id": id, "enabled": on }), move |r| match ok(r, "Could not change that") {
        Ok(_) => ui(|g| {
            let m = g.get_channels();
            for i in 0..m.row_count() {
                if let Some(mut c) = m.row_data(i) { if c.id == id2.as_str() { c.enabled = on; m.set_row_data(i, c); } }
            }
        }),
        Err(e) => kit::toast_error(&format!("Could not change that: {e}").replace("Could not change that: Could not change that", "Could not change that")),
    });
}

fn chat_save() {
    let id = kit::ui().map(|w| w.global::<OpsSettings>().get_chat_id().to_string()).unwrap_or_default();
    post("/api/notify/telegram/chat", json!({ "chatId": id }), |r| match ok(r, "Could not save that") {
        Ok(_) => { kit::toast_ok("Chat ID saved"); render(); }
        Err(e) => kit::toast_error(&e),
    });
}

fn notify_pref(key: &str, mode: &str) {
    if key.is_empty() || !valid_mode(mode) { return; }
    let prefs = with(|s| { s.prefs.insert(key.into(), Value::String(mode.into())); s.prefs.clone() });
    ui(|g| show_notify(g, &prefs));
    post("/api/settings", json!({ "notifyPrefs": prefs }), |r| {
        if let Err(e) = r { kit::toast_error(&format!("Failed to save notification setting: {e}")); }
    });
}

// ─── Sign-in ───

fn method_toggle(method: &str, on: bool) {
    let m = method.to_string();
    let action = if on { "enable" } else { "disable" };
    kit::call({ let m = m.clone(); move |api| api.post("/api/auth/config", &json!({ "action": action, "method": m })) }, move |r| match ok(r, "Could not change that") {
        Ok(_) => {
            ui(|g| match m.as_str() {
                "password" => g.set_pw_on(on),
                "telegram" => g.set_tg_on(on),
                "google" => { let mut o = g.get_google(); o.on = on; g.set_google(o); }
                "github" => { let mut o = g.get_github(); o.on = on; g.set_github(o); }
                _ => {}
            });
            kit::toast_ok(&format!("{} sign-in {}", method_label(&m), if on { "enabled" } else { "disabled" }));
        }
        // The server refuses to leave you with no way in: say so plainly.
        Err(e) => kit::toast_error(&e),
    });
}

fn set_password() {
    let pw = kit::ui().map(|w| w.global::<OpsSettings>().get_new_password().to_string()).unwrap_or_default();
    if pw.is_empty() { kit::toast_error("Type a password first"); return; }
    kit::call(move |api| api.post("/api/auth/config", &json!({ "action": "set-password", "password": pw })), |r| match ok(r, "Could not set that") {
        Ok(v) => {
            ui(|g| {
                g.set_new_password("".into());
                // A new secret with every password: show it at once.
                g.set_enrol_secret(kit::s(&v, "totpSecret").into());
                g.set_pw_set(true);
                g.set_pw_on(true);
            });
            kit::toast_ok("Password set \u{2014} add the new code to your authenticator");
        }
        Err(e) => kit::toast_error(&e),
    });
}

fn tg_save() {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<OpsSettings>();
    let body = json!({ "action": "telegram-save", "botToken": g.get_tg_token().as_str(), "allowedUsername": g.get_tg_user().as_str() });
    kit::call(move |api| api.post("/api/auth/config", &body), |r| match ok(r, "Could not save that") {
        Ok(v) => {
            kit::toast_ok(if kit::truthy(v.get("restartRequired").unwrap_or(&Value::Null)) { "Saved. Restart required for the bot to use it." } else { "Saved" });
            render();
        }
        Err(e) => kit::toast_error(&e),
    });
}

fn tg_clear() {
    kit::confirm(kit::Confirm { title: "Remove the Telegram bot?", message: "Remove the Telegram bot? Telegram sign-in and Telegram notifications both stop after the next restart.", ok: "Remove", danger: true }, |yes| {
        if !yes { return; }
        kit::call(|api| api.post("/api/auth/config", &json!({ "action": "telegram-clear" })), |r| match ok(r, "Could not remove that") {
            Ok(_) => { kit::toast_ok("Removed. Restart required."); render(); }
            Err(e) => kit::toast_error(&e),
        });
    });
}

fn oauth_fields(k: &str) -> (String, String, String) {
    let Some(w) = kit::ui() else { return Default::default() };
    let g = w.global::<OpsSettings>();
    if k == "google" { (g.get_google_id().into(), g.get_google_secret().into(), g.get_google_allowed().into()) }
    else { (g.get_github_id().into(), g.get_github_secret().into(), g.get_github_allowed().into()) }
}

fn oauth_save(k: &str) {
    let (id, secret, allowed) = oauth_fields(k);
    let body = json!({ "action": "oauth-save", "provider": k, "clientId": id, "clientSecret": secret, "allowed": allowed });
    kit::call(move |api| api.post("/api/auth/config", &body), |r| match ok(r, "Could not save that") {
        Ok(_) => { kit::toast_ok("Saved. Run Test to check it works."); render(); }
        Err(e) => kit::toast_error(&e),
    });
}

fn oauth_clear(k: &str) {
    let k = k.to_string();
    let msg = format!("Remove {} sign-in and forget its client ID and secret?", oauth_label(&k));
    kit::confirm(kit::Confirm { title: "Remove sign-in?", message: &msg, ok: "Remove", danger: true }, move |yes| {
        if !yes { return; }
        kit::call(move |api| api.post("/api/auth/config", &json!({ "action": "oauth-clear", "provider": k })), |r| match ok(r, "Could not remove that") {
            Ok(_) => { kit::toast_ok("Removed"); render(); }
            Err(e) => kit::toast_error(&e),
        });
    });
}

fn oauth_line(k: &str, line: &str) {
    ui(|g| {
        if k == "google" { let mut o = g.get_google(); o.test_line = line.into(); g.set_google(o); }
        else { let mut o = g.get_github(); o.test_line = line.into(); g.set_github(o); }
    });
}

/// authOAuthTest: the server makes the provider's address, the system
/// browser opens it, and this asks every 1.5 s what came of it (10 minutes).
fn oauth_test(k: &str) {
    let k = k.to_string();
    let body = json!({ "provider": k, "purpose": "test", "origin": kit::base(), "desktop": true });
    kit::call(move |api| api.post("/api/auth/oauth/start", &body), move |r| {
        let d = match ok(r, "Could not start the test") { Ok(d) => d, Err(e) => { kit::toast_error(&e); return; } };
        open_url(&kit::s(&d, "url"));
        oauth_line(&k, "Waiting for the sign-in in the other window\u{2026}");
        let t = Timer::default();
        let kk = k.clone();
        t.start(TimerMode::Repeated, Duration::from_millis(1500), move || oauth_tick(&kk));
        let old = with(|s| s.oauth_polls.insert(k.clone(), OAuthPoll { timer: t, poll_id: kit::s(&d, "pollId"), until: Instant::now() + Duration::from_secs(600), busy: false }));
        if let Some(o) = old { o.timer.stop(); }
    });
}

fn oauth_stop(k: &str, line: &str) {
    if let Some(p) = with(|s| s.oauth_polls.remove(k)) { p.timer.stop(); }
    if !line.is_empty() { oauth_line(k, line); }
}

fn oauth_tick(k: &str) {
    let Some((pid, late)) = with(|s| s.oauth_polls.get_mut(k).filter(|p| !p.busy).map(|p| { p.busy = true; (p.poll_id.clone(), Instant::now() > p.until) })) else { return };
    if late { oauth_stop(k, "The test timed out."); return; }
    let k = k.to_string();
    kit::call(move |api| api.post("/api/auth/oauth/poll", &json!({ "pollId": pid })), move |r| {
        with(|s| if let Some(p) = s.oauth_polls.get_mut(&k) { p.busy = false; });
        let p = r.unwrap_or(Value::Null);
        match kit::s(&p, "status").as_str() {
            "done" => { oauth_stop(&k, ""); kit::toast_ok(&format!("{} sign-in works ({})", oauth_label(&k), kit::s(&p, "account"))); render(); }
            "failed" => {
                let e = kit::s(&p, "error");
                let acct = kit::s(&p, "account");
                let msg = format!("{}{}", if e.is_empty() { "The test did not pass".to_string() } else { e }, if acct.is_empty() { String::new() } else { format!(" (signed in as {acct})") });
                oauth_stop(&k, &msg);
                kit::toast_error(&msg);
            }
            "expired" => oauth_stop(&k, "The test expired."),
            _ => {}
        }
    });
}

// ─── Server and Claude Code ───

fn server_update(force: bool) {
    let force = force || with(|s| s.srv_checked.map(|t| t.elapsed() > Duration::from_secs(60)).unwrap_or(true));
    if force { with(|s| s.srv_checked = Some(Instant::now())); }
    let path = if force { "/api/update/status?force=1" } else { "/api/update/status" };
    kit::call(move |api| api.get(path), |r| {
        let d = match ok(r, "Failed") {
            Ok(d) => d,
            Err(e) => { ui(|g| { g.set_srv_error(e.into()); g.set_srv_body("error".into()); }); return; }
        };
        let v = server_view(&d, now_secs());
        ui(|g| {
            g.set_srv_version(v.version.clone().into());
            g.set_srv_sub(v.sub.clone().into());
            g.set_srv_act(v.act.clone().into());
            g.set_srv_latest(v.latest.clone().into());
            g.set_srv_channel(v.channel);
            g.set_srv_dev_note(v.dev_note.clone().into());
            g.set_srv_blocker(v.blocker.clone().into());
            g.set_srv_notes_url(v.notes_url.clone().into());
            g.set_srv_log(v.log.clone().into());
            g.set_srv_can_restart(v.can_restart);
            g.set_srv_body("data".into());
        });
        // While it updates the server restarts underneath: keep asking.
        let polling = with(|s| s.srv_poll.as_ref().map(|t| t.running()).unwrap_or(false));
        if v.applying && !polling {
            let t = Timer::default();
            t.start(TimerMode::Repeated, Duration::from_secs(5), || server_update(false));
            with(|s| s.srv_poll = Some(t));
        } else if !v.applying {
            if let Some(t) = with(|s| s.srv_poll.take()) { t.stop(); }
        }
    });
}

fn server_channel(i: i32) {
    let cur = kit::ui().map(|w| w.global::<OpsSettings>().get_srv_channel()).unwrap_or(0);
    if cur == i { return; }
    let go = move || {
        let want = if i == 1 { "dev" } else { "production" };
        post("/api/update/channel", json!({ "channel": want }), move |r| match ok(r, "Could not switch") {
            Ok(_) => { kit::toast_ok(if i == 1 { "Server follows dev releases" } else { "Server follows production releases" }); server_update(false); }
            Err(e) => kit::toast_error(&e),
        });
    };
    if i == 1 { confirm_dev("this server", go); } else { go(); }
}

/// confirmDevChannel: leaving production is the one-way step.
fn confirm_dev(what: &str, go: impl FnOnce() + 'static) {
    let msg = format!("Switch {what} to dev releases?\n\nDev builds are previews of what is coming and may be less stable.\n\nYou cannot go back to production until a production release newer than your dev build is published: Crundi never installs an older version. A newer production release is always offered on the dev channel too.");
    kit::confirm(kit::Confirm { title: "Switch to dev releases?", message: &msg, ok: "Switch", danger: false }, move |yes| if yes { go() });
}

fn server_apply() {
    kit::confirm(kit::Confirm { title: "Update the server now?", message: "Update the server now?\n\nThis restarts Crundi when it finishes, which ends every running chat and terminal on it.", ok: "Update", danger: false }, |yes| {
        if !yes { return; }
        ui(|g| g.set_srv_body("starting".into()));
        kit::call(|api| api.post("/api/update/apply", &json!({})), |r| {
            match ok(r, "Could not start the update") {
                Ok(d) => kit::toast_ok(&format!("Updating to {}", kit::s(&d, "version"))),
                Err(e) => kit::toast_error(&e),
            }
            server_update(false);
        });
    });
}

fn restart_server() {
    kit::confirm(kit::Confirm { title: "Restart the server?", message: "Restart the server?\n\nThis ends every running chat and terminal on it. It comes back on its own in a few seconds.", ok: "Restart", danger: true }, |yes| {
        if !yes { return; }
        kit::call(|api| api.post("/api/restart", &json!({})), |r| match r {
            Ok(v) if v.get("ok") == Some(&Value::Bool(false)) => kit::toast_error(&{ let e = kit::s(&v, "error"); if e.is_empty() { "Could not restart".into() } else { e } }),
            // The connection dropping is the restart: not an error.
            _ => kit::toast_ok("Restarting\u{2026}"),
        });
    });
}

fn claude_update(force: bool) {
    let path = if force { "/api/claude-update/status?force=1" } else { "/api/claude-update/status" };
    kit::call(move |api| api.get(path), |r| {
        let c = match ok(r, "Failed") {
            Ok(d) => d.get("claude").cloned().unwrap_or(Value::Null),
            Err(e) => { ui(|g| { g.set_cc_error(e.into()); g.set_cc_body("error".into()); }); return; }
        };
        let v = claude_view(&c);
        ui(|g| {
            g.set_cc_version(v.version.into());
            g.set_cc_sub(v.sub.into());
            g.set_cc_act(v.act.into());
            g.set_cc_latest(v.latest.into());
            g.set_cc_beyond(v.beyond);
            g.set_cc_tested(v.tested.into());
            g.set_cc_tested_warn(v.tested_warn.into());
            g.set_cc_blocker(v.blocker.into());
            g.set_cc_sudo(v.sudo.into());
            g.set_cc_last(v.last.into());
            g.set_cc_body("data".into());
        });
    });
}

fn claude_apply() {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<OpsSettings>();
    let version = g.get_cc_latest().to_string();
    let beyond = g.get_cc_beyond();
    let go = move || {
        ui(|g| g.set_cc_body("installing".into()));
        let body = json!({ "version": version, "allowBeyondTested": beyond });
        kit::call(move |api| api.post("/api/claude-update/apply", &body), |r| {
            match ok(r, "Update failed") {
                Ok(d) => kit::toast_ok(&format!("Claude Code updated to {}", kit::s(&d, "installed"))),
                Err(e) => kit::toast_error(&e),
            }
            claude_update(true);
        });
    };
    if beyond {
        let msg = format!("Claude Code {} is newer than the version this Crundi build was tested with.\n\nClaude Code is what every chat and terminal runs on, so a problem there affects all of them.\n\nInstall it anyway?", g.get_cc_latest());
        kit::confirm(kit::Confirm { title: "Install anyway?", message: &msg, ok: "Install", danger: false }, move |yes| if yes { go() });
    } else { go(); }
}

// ─── Session ───

fn sessions_load() {
    ui(|g| g.set_sess_state("loading".into()));
    kit::call(|api| api.get("/api/auth/sessions"), |r| match ok(r, "Failed") {
        Ok(d) => {
            let list = d.get("sessions").and_then(|s| s.as_array()).cloned().unwrap_or_default();
            let rows = session_rows(&list, now_secs());
            let others = rows.iter().filter(|r| !r.current).count() as i32;
            ui(|g| {
                g.set_sessions(slint::ModelRc::new(slint::VecModel::from(rows.into_iter().map(|r| crate::OpsSettingsSession {
                    id: r.id.into(), label: r.label.into(), current: r.current, desc: r.desc.into(),
                }).collect::<Vec<_>>())));
                g.set_sess_others(others);
                g.set_sess_state(if list.is_empty() { "empty" } else { "list" }.into());
            });
        }
        Err(e) => ui(|g| { g.set_sess_error(e.into()); g.set_sess_state("error".into()); }),
    });
}

fn sess_revoke(id: String) {
    if id.is_empty() { return; }
    post("/api/auth/sessions/revoke", json!({ "id": id }), |r| {
        match ok(r, "Failed") { Ok(_) => kit::toast_ok("Session revoked"), Err(e) => kit::toast_error(&e) }
        sessions_load();
    });
}

fn sess_revoke_others() {
    post("/api/auth/sessions/revoke", json!({ "others": true }), |r| {
        match ok(r, "Failed") {
            Ok(d) => {
                let n = d.get("revoked").and_then(|n| n.as_i64()).unwrap_or(0);
                kit::toast_ok(&if n > 0 { format!("Signed out {n} other session(s)") } else { "Nothing else was signed in".into() });
            }
            Err(e) => kit::toast_error(&e),
        }
        sessions_load();
    });
}

// ─── Phone app ───

fn push_load() {
    ui(|g| g.set_push_state("checking".into()));
    kit::call(|api| api.get("/api/push/mobile"), |r| match r {
        Ok(s) if s.get("ok") == Some(&Value::Bool(true)) => ui(|g| {
            g.set_push_line(push_line(&s).into());
            let has_key = kit::truthy(s.get("hasKey").unwrap_or(&Value::Null));
            let url = s.pointer("/contact/url").and_then(|u| u.as_str()).unwrap_or("").to_string();
            let tg = s.pointer("/contact/telegram").and_then(|u| u.as_str()).unwrap_or("").to_string();
            g.set_push_contact(if has_key || url.is_empty() { "".into() } else if !tg.is_empty() { format!("Message @{tg}").into() } else { "Ask for a key".into() });
            g.set_push_contact_url(url.into());
            g.set_push_error(kit::s(&s, "lastError").into());
            g.set_push_has_key(has_key);
            g.set_push_key("".into());
            g.set_relay_summary(relay_summary(&s).into());
            let custom = kit::truthy(s.get("customRelay").unwrap_or(&Value::Null));
            g.set_relay_custom(custom);
            g.set_relay_url(if custom { kit::s(&s, "relayUrl").into() } else { "".into() });
            g.set_push_state("data".into());
        }),
        _ => ui(|g| g.set_push_state("none".into())),
    });
}

fn push_save() {
    let key = kit::ui().map(|w| w.global::<OpsSettings>().get_push_key().trim().to_string()).unwrap_or_default();
    if key.is_empty() { return; }
    kit::call(move |api| api.post("/api/push/mobile/key", &json!({ "key": key })), |r| match r {
        Ok(d) if d.get("ok") == Some(&Value::Bool(true)) => {
            if kit::truthy(d.get("accepted").unwrap_or(&Value::Null)) {
                let l = kit::s(&d, "label");
                kit::toast_ok(&format!("Push key saved{}", if l.is_empty() { String::new() } else { format!(" ({l})") }));
            } else {
                let e = kit::s(&d, "error");
                kit::toast_error(if e.is_empty() { "Saved, but the relay did not accept it" } else { &e });
            }
            push_load();
        }
        Ok(d) => { let e = kit::s(&d, "error"); kit::toast_error(if e.is_empty() { "Could not save the key" } else { &e }); }
        Err(_) => kit::toast_error("Could not reach the server"),
    });
}

fn push_mobile_test() {
    kit::call(|api| api.post("/api/push/mobile/test", &json!({})), |r| {
        match r {
            Ok(d) if d.get("ok") == Some(&Value::Bool(true)) => {
                if kit::truthy(d.get("sent").unwrap_or(&Value::Null)) { kit::toast_ok("Test notification sent") }
                else { let n = kit::s(&d, "note"); kit::toast_ok(if n.is_empty() { "The key is good." } else { &n }) }
            }
            Ok(d) => { let e = kit::s(&d, "error"); kit::toast_error(if e.is_empty() { "The test did not go through" } else { &e }) }
            Err(_) => { kit::toast_error("Could not reach the server"); return; }
        }
        push_load();
    });
}

fn relay_save(url: String) {
    kit::call(move |api| api.post("/api/push/mobile/relay", &json!({ "relayUrl": url })), |r| match r {
        Ok(d) if d.get("ok") == Some(&Value::Bool(true)) => {
            if d.get("reachable") == Some(&Value::Bool(false)) {
                let e = kit::s(&d, "error");
                kit::toast_error(if e.is_empty() { "Saved, but that relay did not answer" } else { &e });
            } else { kit::toast_ok("Relay changed"); }
            push_load();
        }
        Ok(d) => { let e = kit::s(&d, "error"); kit::toast_error(if e.is_empty() { "Could not change the relay" } else { &e }); }
        Err(_) => kit::toast_error("Could not reach the server"),
    });
}

/// crundi-phone.js connect(): one dialog at a time.
fn pair_start() {
    if kit::ui().map(|w| w.global::<OpsSettings>().get_pair_open()).unwrap_or(false) { return; }
    ui(|g| { g.set_pair_open(true); g.set_modal(true); });
    pair_begin();
}

fn pair_stop() {
    if let Some(p) = with(|s| s.pair.take()) { p.tick.stop(); p.poll.stop(); }
}

fn pair_begin() {
    pair_stop();
    ui(|g| {
        g.set_pair_again(false);
        g.set_pair_line("Getting a code\u{2026}".into());
        g.set_pair_done(false);
        g.set_pair_small("".into());
        g.set_pair_has_image(false);
        g.set_pair_pic_text("".into());
        g.set_pair_swap("".into());
        g.set_pair_copy("".into());
    });
    let origin = kit::base();
    kit::call(move |api| api.post("/api/pair/start", &json!({ "origin": origin })), |r| {
        if !kit::ui().map(|w| w.global::<OpsSettings>().get_pair_open()).unwrap_or(false) { return; }
        let d = match r {
            Ok(d) if d.get("ok") == Some(&Value::Bool(true)) => d,
            Ok(d) => {
                let e = kit::s(&d, "error");
                ui(|g| { g.set_pair_line(if e.is_empty() { "Could not get a pairing code.".into() } else { e.into() }); g.set_pair_again(true); });
                return;
            }
            Err(_) => { ui(|g| { g.set_pair_line("Could not reach the server.".into()); g.set_pair_again(true); }); return; }
        };
        let left = d.get("expiresIn").and_then(|n| n.as_i64()).unwrap_or(300);
        let tick = Timer::default();
        tick.start(TimerMode::Repeated, Duration::from_secs(1), || {
            with(|s| if let Some(p) = s.pair.as_mut() { p.left = (p.left - 1).max(0); });
            pair_paint();
        });
        let poll = Timer::default();
        let code = kit::s(&d, "code");
        let done_ms = d.get("doneMs").and_then(|n| n.as_u64()).unwrap_or(1500);
        poll.start(TimerMode::Repeated, Duration::from_millis(1500), move || {
            let Some(code) = with(|s| s.pair.as_ref().filter(|p| !p.paired && !p.code.is_empty()).map(|p| p.code.clone())) else { return };
            kit::call(move |api| api.post("/api/pair/status", &json!({ "code": code })), move |r| {
                let Ok(s) = r else { return };
                if s.get("ok") != Some(&Value::Bool(true)) || with(|st| st.pair.is_none()) { return; }
                match kit::s(&s, "state").as_str() {
                    "paired" => pair_succeed(s.get("device").cloned().unwrap_or(Value::Null), done_ms),
                    "expired" => {
                        with(|st| if let Some(p) = st.pair.as_mut() { p.tick.stop(); p.poll.stop(); p.code.clear(); });
                        ui(|g| {
                            g.set_pair_has_image(false);
                            g.set_pair_pic_text("Expired".into());
                            g.set_pair_line("That code ran out before a phone used it.".into());
                            g.set_pair_small("".into());
                            g.set_pair_swap("".into());
                            g.set_pair_copy("".into());
                            g.set_pair_again(true);
                        });
                    }
                    _ => {}
                }
            });
        });
        let pair = Pair {
            code, link: kit::s(&d, "link"), qr: kit::s(&d, "qr"), mark: kit::s(&d, "mark"), showing_qr: false,
            note: String::new(), warn: if kit::truthy(d.get("insecure").unwrap_or(&Value::Null)) { "This server is not using https. ".into() } else { String::new() },
            left, paired: false, tick, poll,
        };
        let has_link = !pair.link.is_empty();
        with(|s| s.pair = Some(pair));
        ui(|g| {
            g.set_pair_copy(if has_link { "Copy the pairing link instead".into() } else { "".into() });
            g.set_pair_line("Open the Crundi app on your phone and point it here.".into());
        });
        pair_show();
    });
}

/// The picture: the Crundi code, or the QR code (the server's finished SVG).
fn pair_show() {
    let Some((svg, has_mark, showing_qr)) = with(|s| s.pair.as_ref().map(|p| (if p.showing_qr || p.mark.is_empty() { p.qr.clone() } else { p.mark.clone() }, !p.mark.is_empty(), p.showing_qr))) else { return };
    let img = svg_image(&svg);
    with(|s| if let Some(p) = s.pair.as_mut() {
        p.note = if has_mark { String::new() } else { "This address is too long for the Crundi code, so this is a QR code.".into() };
    });
    ui(|g| {
        match img {
            Some(i) => { g.set_pair_image(i); g.set_pair_has_image(true); }
            None => { g.set_pair_has_image(false); g.set_pair_pic_text("".into()); }
        }
        g.set_pair_swap(if !has_mark { "".into() } else if showing_qr { "Show the Crundi code".into() } else { "Show a QR code instead".into() });
    });
    pair_paint();
}

fn svg_image(svg: &str) -> Option<slint::Image> {
    if svg.is_empty() { return None; }
    slint::Image::load_from_svg_data(svg.as_bytes()).ok()
}

fn pair_paint() {
    let Some((note, warn, left, paired)) = with(|s| s.pair.as_ref().map(|p| (p.note.clone(), p.warn.clone(), p.left, p.paired))) else { return };
    if paired { return; }
    ui(|g| g.set_pair_small(format!("{}{}{}", if note.is_empty() { String::new() } else { format!("{note} ") }, warn, expires_in(left)).into()));
}

fn pair_succeed(device: Value, done_ms: u64) {
    with(|s| if let Some(p) = s.pair.as_mut() { p.paired = true; p.tick.stop(); p.poll.stop(); p.showing_qr = false; });
    pair_show();
    let name = { let n = kit::s(&device, "name"); if n.is_empty() { "Phone".to_string() } else { n } };
    ui(|g| {
        g.set_pair_swap("".into());
        g.set_pair_copy("".into());
        g.set_pair_small("".into());
        g.set_pair_line(format!("{name} connected").into());
        g.set_pair_done(true);
    });
    kit::toast_ok(&format!("{name} connected"));
    after(done_ms + 1800, pair_close);
}

fn pair_copy() {
    let Some(link) = with(|s| s.pair.as_ref().map(|p| p.link.clone())) else { return };
    if link.is_empty() { return; }
    let good = copy(&link);
    ui(|g| g.set_pair_copy(if good { "Link copied. Paste it in the app." } else { "Could not copy the link." }.into()));
    after(2600, || if with(|s| s.pair.is_some()) { ui(|g| g.set_pair_copy("Copy the pairing link instead".into())); });
}

fn pair_close() {
    if !kit::ui().map(|w| w.global::<OpsSettings>().get_pair_open()).unwrap_or(false) { return; }
    let p = with(|s| s.pair.take());
    let mut paired = false;
    if let Some(p) = p {
        p.tick.stop(); p.poll.stop();
        paired = p.paired;
        // A code left on a closed dialog is a login nobody is watching: take it back.
        if !p.code.is_empty() && !p.paired { post("/api/pair/cancel", json!({ "code": p.code }), |_| {}); }
    }
    ui(|g| { g.set_pair_open(false); g.set_modal(false); });
    // A new phone is a new signed-in device.
    if paired && kit::ui().map(|w| w.global::<OpsSettings>().get_sess_open()).unwrap_or(false) { sessions_load(); }
}

// ─── Cru ───

fn voice_load() {
    kit::call(|api| api.get("/api/voice/settings"), |r| {
        let s = match r { Ok(s) if s.get("ok") == Some(&Value::Bool(true)) => s, _ => { ui(|g| g.set_vs_shown(false)); return; } };
        voice_show(&s);
    });
}

fn nums(list: &[f64]) -> Vec<Value> { list.iter().map(|n| json!(n)).collect() }

fn voice_show(s: &Value) {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<OpsSettings>();
    let f = |k: &str| s.get(k).and_then(|v| v.as_f64());
    let mut values: HashMap<&'static str, Vec<Value>> = HashMap::new();
    let has_brain = !kit::s(s, "brain").is_empty();
    g.set_vs_has_brain(has_brain);
    g.set_vs_brain(if kit::s(s, "brain") == "claude-code" { 1 } else { 0 });
    values.insert("brain", vec![json!("api"), json!("claude-code")]);
    let models = s.get("models").and_then(|m| m.as_array()).cloned().unwrap_or_default();
    let model = kit::s(s, "model");
    g.set_vs_models(sv(&models.iter().map(|m| kit::s(m, "label")).collect::<Vec<_>>()));
    g.set_vs_model(models.iter().position(|m| kit::s(m, "id") == model).map(|i| i as i32).unwrap_or(-1));
    values.insert("model", models.iter().map(|m| json!(kit::s(m, "id"))).collect());
    let efforts = s.get("efforts").and_then(|e| e.as_array()).cloned().unwrap_or_default();
    let effort = kit::s(s, "effort");
    let has_effort = !efforts.is_empty() && !effort.is_empty();
    g.set_vs_has_effort(has_effort);
    if has_effort {
        let cur = models.iter().find(|m| kit::s(m, "id") == model).cloned().unwrap_or(Value::Null);
        let takes = cur.get("efforts").and_then(|e| e.as_array()).filter(|a| !a.is_empty()).cloned().unwrap_or(efforts);
        let ids: Vec<String> = takes.iter().map(|e| e.as_str().unwrap_or("").to_string()).collect();
        g.set_vs_efforts(sv(&ids.iter().map(|e| effort_label(e)).collect::<Vec<_>>()));
        g.set_vs_effort(ids.iter().position(|e| *e == effort).map(|i| i as i32).unwrap_or(-1));
        g.set_vs_effort_hint(effort_hint(&cur).into());
        values.insert("effort", ids.iter().map(|e| json!(e)).collect());
    }
    let pick = |g: &OpsSettings, list: &[f64], cur: f64, label: &dyn Fn(f64) -> String| -> (Vec<f64>, slint::ModelRc<SharedString>, i32) {
        let c = choices(list, cur);
        let _ = g;
        let labels: Vec<String> = c.iter().map(|n| label(*n)).collect();
        let i = index_of(&c, cur);
        (c, sv(&labels), i)
    };
    let cidle = f("cliIdleMinutes").unwrap_or(30.0);
    let (c, m, i) = pick(&g, &[5.0, 10.0, 15.0, 30.0, 60.0, 120.0], cidle, &|n| format!("{} minutes idle", js_num(n)));
    g.set_vs_cidles(m); g.set_vs_cidle(i); values.insert("cliIdleMinutes", nums(&c));
    let rate = f("speechRate").filter(|r| *r > 0.0).unwrap_or(1.0);
    let (c, m, i) = pick(&g, &[0.85, 1.0, 1.15, 1.3, 1.5], rate, &rate_label);
    g.set_vs_rates(m); g.set_vs_rate(i); values.insert("speechRate", nums(&c));
    g.set_vs_has_volume(s.get("volume").is_some());
    if let Some(vol) = f("volume") {
        let (c, m, i) = pick(&g, &[25.0, 40.0, 55.0, 70.0, 85.0, 100.0], vol, &|n| format!("{}%", js_num(n)));
        g.set_vs_vols(m); g.set_vs_vol(i); values.insert("volume", nums(&c));
    }
    let pause = f("pauseMs").filter(|p| *p > 0.0).unwrap_or(2000.0);
    let (c, m, i) = pick(&g, &[1500.0, 2000.0, 2500.0, 3000.0, 4000.0], pause, &|n| format!("{} seconds", js_num(n / 1000.0)));
    g.set_vs_pauses(m); g.set_vs_pause(i); values.insert("pauseMs", nums(&c));
    let sclose = f("screenCloseSeconds").unwrap_or(0.0);
    g.set_vs_has_sclose(sclose > 0.0);
    if sclose > 0.0 {
        let (c, m, i) = pick(&g, &[30.0, 45.0, 60.0, 120.0, 300.0], sclose, &|n| format!("{} of silence", sec_label(n as i64)));
        g.set_vs_scloses(m); g.set_vs_sclose(i); values.insert("screenCloseSeconds", nums(&c));
    }
    g.set_vs_has_admin(s.get("adminMode").is_some());
    g.set_vs_admin(kit::truthy(s.get("adminMode").unwrap_or(&Value::Null)));
    g.set_vs_admin_hint(admin_hint(s).into());
    let away = s.get("away").filter(|a| a.is_object());
    let classes = s.get("awayClasses").and_then(|c| c.as_array()).cloned();
    g.set_vs_has_away(away.is_some() && classes.is_some());
    let allow: Vec<String> = away.and_then(|a| a.get("allow")).and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    if let (Some(a), Some(cl)) = (away, classes) {
        g.set_vs_away(kit::truthy(a.get("on").unwrap_or(&Value::Null)));
        g.set_vs_away_classes(slint::ModelRc::new(slint::VecModel::from(cl.iter().map(|c| crate::OpsSettingsCheck {
            id: kit::s(c, "id").into(), label: kit::s(c, "label").into(), checked: allow.contains(&kit::s(c, "id")),
        }).collect::<Vec<_>>())));
    }
    g.set_vs_spend(spend_line(s).into());
    g.set_vs_limit(js_num(f("monthlyLimitUsd").unwrap_or(0.0)).into());
    g.set_vs_bday(js_num(f("billingDay").filter(|n| *n > 0.0).unwrap_or(1.0)).into());
    g.set_vs_dlimit(js_num(f("deepgramLimitUsd").unwrap_or(0.0)).into());
    g.set_vs_dday(js_num(f("deepgramBillingDay").filter(|n| *n > 0.0).unwrap_or(1.0)).into());
    g.set_vs_akey("".into()); g.set_vs_dkey("".into()); g.set_vs_xkey("".into());
    g.set_vs_akey_set(kit::truthy(s.get("anthropicKeySet").unwrap_or(&Value::Null)));
    g.set_vs_dkey_set(kit::truthy(s.get("deepgramKeySet").unwrap_or(&Value::Null)));
    g.set_vs_xkey_set(kit::truthy(s.get("anthropicAdminKeySet").unwrap_or(&Value::Null)));
    g.set_vs_shown(true);
    with(|st| { st.voice.values = values; st.voice.away_allow = allow; });
}

/// Save a part of Cru's settings; the status line says how it went.
fn voice_save(body: Value, note: String, then: impl FnOnce(bool) + Send + 'static) {
    kit::call(move |api| api.post("/api/voice/settings", &body), move |r| {
        let good = matches!(&r, Ok(v) if v.get("ok") == Some(&Value::Bool(true)));
        let text = if good { note } else { match &r { Ok(v) if !kit::s(v, "error").is_empty() => kit::s(v, "error"), _ => "Not saved".into() } };
        ui(|g| { g.set_vs_status(text.into()); g.set_vs_status_bad(!good); });
        then(good);
    });
}

fn voice_pick(field: &str, i: i32) {
    let Some((key, v)) = with(|s| s.voice.values.iter().find(|(k, _)| **k == field).and_then(|(k, l)| l.get(i as usize).map(|v| (*k, v.clone())))) else { return };
    let note = match key {
        "brain" => if v == "claude-code" { "Cru answers through Claude Code now".to_string() } else { "Cru answers through the Claude API now".into() },
        "model" => "Model saved. It applies from the next conversation.".into(),
        "effort" => "Thinking effort saved. It applies from the next answer.".into(),
        "speechRate" => format!("Cru talks at {}", rate_label(v.as_f64().unwrap_or(1.0))),
        "volume" => format!("Cru talks at {}%", js_num(v.as_f64().unwrap_or(0.0))),
        _ => "Saved".into(),
    };
    ui(|g| match key {
        "brain" => g.set_vs_brain(i), "model" => g.set_vs_model(i), "effort" => g.set_vs_effort(i),
        "cliIdleMinutes" => g.set_vs_cidle(i), "speechRate" => g.set_vs_rate(i), "volume" => g.set_vs_vol(i),
        "pauseMs" => g.set_vs_pause(i), "screenCloseSeconds" => g.set_vs_sclose(i), _ => {}
    });
    { let mut b = Map::new(); b.insert(key.to_string(), v); voice_save(Value::Object(b), note, |_| {}); }
}

fn voice_admin(want: bool) {
    ui(|g| g.set_vs_admin(want));
    let note = if want { "Admin mode is on: Cru can run any command, also on its own; spoken requests need your voice" } else { "Admin mode is off" };
    voice_save(json!({ "adminMode": want }), note.into(), move |good| if !good { ui(|g| g.set_vs_admin(!want)); });
}

fn voice_away(on: Option<bool>, class: Option<(String, bool)>) {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<OpsSettings>();
    let was = g.get_vs_away();
    if let Some(on) = on { g.set_vs_away(on); }
    if let Some((id, c)) = &class {
        let m = g.get_vs_away_classes();
        for i in 0..m.row_count() { if let Some(mut r) = m.row_data(i) { if r.id == id.as_str() { r.checked = *c; m.set_row_data(i, r); } } }
    }
    let m = g.get_vs_away_classes();
    let allow: Vec<String> = (0..m.row_count()).filter_map(|i| m.row_data(i)).filter(|r| r.checked).map(|r| r.id.to_string()).collect();
    let now_on = g.get_vs_away();
    let note = match on { Some(true) => "Cru may now act on its own, as ticked", Some(false) => "Cru asks before every change again", None => "Saved" };
    voice_save(json!({ "away": { "on": now_on, "allow": allow } }), note.into(), move |good| {
        if !good && on.is_some() { ui(|g| g.set_vs_away(was)); }
    });
}

fn voice_away_stop() {
    kit::confirm(kit::Confirm { title: "Stop everything?", message: "Stop everything Cru does on its own? Its tickers, triggers and jobs pause until switched on again.", ok: "Stop everything", danger: true }, |yes| {
        if !yes { return; }
        post("/api/voice/away/stop", json!({ "everything": true }), |r| match r {
            Ok(d) if d.get("ok") == Some(&Value::Bool(true)) => ui(|g| {
                g.set_vs_away(false);
                g.set_vs_away_note(format!("Stopped: {} wake-ups paused, {} jobs paused", d.get("wakeupsPaused").and_then(|n| n.as_i64()).unwrap_or(0), d.get("jobsPaused").and_then(|n| n.as_i64()).unwrap_or(0)).into());
                g.set_vs_away_bad(false);
            }),
            other => {
                let e = match &other { Ok(d) => kit::s(d, "error"), Err(_) => String::new() };
                ui(|g| { g.set_vs_away_note(if e.is_empty() { "Could not stop".into() } else { e.into() }); g.set_vs_away_bad(true); });
            }
        });
    });
}

fn voice_save_spend() {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<OpsSettings>();
    let n = |s: SharedString| s.trim().parse::<f64>().unwrap_or(0.0);
    let day = |s: SharedString| { let d = s.trim().parse::<f64>().unwrap_or(0.0); if d > 0.0 { d } else { 1.0 } };
    let body = json!({ "monthlyLimitUsd": n(g.get_vs_limit()), "deepgramLimitUsd": n(g.get_vs_dlimit()), "billingDay": day(g.get_vs_bday()), "deepgramBillingDay": day(g.get_vs_dday()) });
    post("/api/voice/settings", body, |r| {
        let good = matches!(&r, Ok(v) if v.get("ok") == Some(&Value::Bool(true)));
        let e = match &r { Ok(v) => kit::s(v, "error"), Err(_) => String::new() };
        ui(|g| { g.set_vs_spend_note(if good { "Saved".into() } else if e.is_empty() { "Not saved".into() } else { e.into() }); g.set_vs_spend_bad(!good); });
    });
}

fn voice_save_keys() {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<OpsSettings>();
    let mut body = Map::new();
    for (v, k) in [(g.get_vs_akey(), "anthropicKey"), (g.get_vs_dkey(), "deepgramKey"), (g.get_vs_xkey(), "anthropicAdminKey")] {
        let v = v.trim().to_string();
        if !v.is_empty() { body.insert(k.into(), Value::String(v)); }
    }
    if body.is_empty() { g.set_vs_keys_note("Type a key to change it".into()); g.set_vs_keys_bad(false); return; }
    post("/api/voice/settings", Value::Object(body), |r| match r {
        Ok(v) if v.get("ok") == Some(&Value::Bool(true)) => { ui(|g| g.set_vs_keys_note("".into())); voice_load(); }
        other => {
            let e = match &other { Ok(d) => kit::s(d, "error"), Err(_) => String::new() };
            ui(|g| { g.set_vs_keys_note(if e.is_empty() { "Not saved".into() } else { e.into() }); g.set_vs_keys_bad(true); });
        }
    });
}

// ─── The Desktop App (this client) ───

fn desktop_show(g: &OpsSettings) {
    let l = local::load();
    g.set_launch(local::launch());
    g.set_launch_hint(local::launch_hint().into());
    g.set_auto_update(l.auto_update.unwrap_or(true));
    g.set_upd_channel(if l.channel == "dev" { 1 } else { 0 });
    if !g.get_upd_avail() { g.set_upd_status(format!("Up to date \u{2014} v{}", env!("CARGO_PKG_VERSION")).into()); }
}

fn desktop_channel(i: i32) {
    let cur = kit::ui().map(|w| w.global::<OpsSettings>().get_upd_channel()).unwrap_or(0);
    if cur == i { return; }
    let go = move || {
        local::update(|l| l.channel = if i == 1 { "dev".into() } else { "production".into() });
        ui(|g| g.set_upd_channel(i));
    };
    if i == 1 { confirm_dev("the desktop app", go); } else { go(); }
}

// ─── Helpers ───

fn open_url(url: &str) {
    if !(url.starts_with("http://") || url.starts_with("https://")) { return; }
    with(|s| s.data["opened"] = Value::String(url.to_string()));
    // Under the test driver it is only recorded.
    if std::env::var_os("CRUNDI_TEST_PORT").is_some() { return; }
    let u = url.to_string();
    std::thread::spawn(move || { let _ = open::that(&u); });
}

fn copy(text: &str) -> bool {
    if text.is_empty() { return false; }
    let Ok(mut cb) = arboard::Clipboard::new() else { return false };
    if cb.set_text(text.to_string()).is_err() { return false; }
    crate::app::keep_clipboard(cb);
    true
}

// ─── The test driver ───

/// The driver's "ops:settings-*" actions: they call what the controls call.
pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let w = kit::ui()?;
    let g = w.global::<OpsSettings>();
    match name {
        "settings-reload" => g.invoke_reload(),
        // Open or close a section by its key (server, cc, signin…).
        "settings-open" => {
            let on = a(1) != "false";
            match a(0).as_str() {
                "server" => g.set_open_server(on), "cc" => g.set_open_cc(on), "whatsnew" => g.set_open_whatsnew(on),
                "sysprompt" => g.set_open_sysprompt(on), "skills" => g.set_open_skills(on), "usage" => g.set_open_usage(on),
                "voice" => g.set_open_voice(on), "channels" => g.set_open_channels(on), "notify" => g.set_open_notify(on),
                "signin" => g.set_open_signin(on), "phone" => g.set_open_phone(on), "session" => g.set_open_session(on),
                "config" => g.set_open_config(on), "terminal" => g.set_open_terminal(on), "backup" => g.set_open_backup(on),
                "desktop" => g.set_open_desktop(on),
                _ => return Some(json!({ "ok": false, "error": "no such section" })),
            }
        }
        // Type into a field, as a person would.
        "settings-field" => {
            let v: SharedString = a(1).into();
            match a(0).as_str() {
                "projects-dir" => g.set_projects_dir(v), "web-port" => g.set_web_port(v), "tunnel-url" => g.set_tunnel_url(v),
                "sysprompt" => g.set_sysprompt(v), "proj-sysprompt" => g.set_proj_sysprompt(v),
                "new-password" => g.set_new_password(v), "tg-token" => g.set_tg_token(v), "tg-user" => g.set_tg_user(v),
                "chat-id" => g.set_chat_id(v), "google-id" => g.set_google_id(v), "google-secret" => g.set_google_secret(v),
                "google-allowed" => g.set_google_allowed(v), "push-key" => g.set_push_key(v), "autopark" => g.set_autopark(v),
                "vs-limit" => g.set_vs_limit(v), "vs-akey" => g.set_vs_akey(v),
                _ => return Some(json!({ "ok": false, "error": "no such field" })),
            }
        }
        // Press a control: the callback's name and its arguments.
        "settings-do" => match a(0).as_str() {
            "save-config" => g.invoke_save_config(),
            "save-prompts" => g.invoke_save_prompts(),
            "srv-check" => g.invoke_srv_check(),
            "srv-set-channel" => g.invoke_srv_set_channel(a(1).parse().unwrap_or(0)),
            "cc-check" => g.invoke_cc_check(),
            "restart" => g.invoke_restart(),
            "whats-new" => g.invoke_whats_new(),
            "notify-pref" => g.invoke_notify_pref(a(1).into(), a(2).into()),
            "channel-toggle" => g.invoke_channel_toggle(a(1).into(), a(2) == "true"),
            "chat-save" => g.invoke_chat_save(),
            "goto-signin-telegram" => g.invoke_goto_signin_telegram(),
            "set-password" => g.invoke_set_password(),
            "method-toggle" => g.invoke_method_toggle(a(1).into(), a(2) == "true"),
            "tg-save" => g.invoke_tg_save(),
            "tg-clear" => g.invoke_tg_clear(),
            "oauth-save" => g.invoke_oauth_save(a(1).into()),
            "oauth-test" => g.invoke_oauth_test(a(1).into()),
            "oauth-clear" => g.invoke_oauth_clear(a(1).into()),
            "oauth-copy" => g.invoke_oauth_copy(a(1).into()),
            "phone-connect" => g.invoke_phone_connect(),
            "pair-close" => g.invoke_pair_close(),
            "push-save" => g.invoke_push_save(),
            "sess-toggle" => g.invoke_sess_toggle(),
            "sess-revoke" => g.invoke_sess_revoke(a(1).into()),
            "sess-revoke-others" => g.invoke_sess_revoke_others(),
            "set-newline-key" => g.invoke_set_newline_key(a(1).parse().unwrap_or(0)),
            "autopark-commit" => g.invoke_autopark_commit(g.get_autopark()),
            "vs-pick" => g.invoke_vs_pick(a(1).into(), a(2).parse().unwrap_or(0)),
            "vs-save-spend" => g.invoke_vs_save_spend(),
            "vs-save-keys" => g.invoke_vs_save_keys(),
            "vs-set-admin" => g.invoke_vs_set_admin(a(1) == "true"),
            "launch-toggle" => g.invoke_launch_toggle(a(1) == "true"),
            "update-toggle" => g.invoke_update_toggle(a(1) == "true"),
            "upd-set-channel" => g.invoke_upd_set_channel(a(1).parse().unwrap_or(0)),
            "update-check" => g.invoke_update_check(),
            _ => return Some(json!({ "ok": false, "error": "no such control" })),
        },
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// This screen's part of the area's props (under "settings").
pub fn props() -> Value {
    let Some(w) = kit::ui() else { return json!({}) };
    let g = w.global::<OpsSettings>();
    let opened = with(|s| kit::s(&s.data, "opened"));
    let rows = |m: slint::ModelRc<crate::OpsSettingsNotify>| (0..m.row_count()).filter_map(|i| m.row_data(i)).filter(|r| r.cat.is_empty())
        .map(|r| (r.key.to_string(), Value::String(r.mode.to_string()))).collect::<Map<_, _>>();
    let chans = g.get_channels();
    let sess = g.get_sessions();
    let oauth = |o: crate::OpsSettingsOAuth| json!({ "on": o.on, "canOn": o.can_on, "status": o.status.as_str(), "configured": o.configured, "url": o.url.as_str(), "testLine": o.test_line.as_str() });
    json!({
        "state": g.get_state().as_str(),
        "error": g.get_error().as_str(),
        "open": {
            "server": g.get_open_server(), "cc": g.get_open_cc(), "signin": g.get_open_signin(), "channels": g.get_open_channels(),
            "notify": g.get_open_notify(), "session": g.get_open_session(), "config": g.get_open_config(), "terminal": g.get_open_terminal(),
            "voice": g.get_open_voice(), "phone": g.get_open_phone(), "desktop": g.get_open_desktop(), "sysprompt": g.get_open_sysprompt(),
        },
        "server": { "body": g.get_srv_body().as_str(), "version": g.get_srv_version().as_str(), "sub": g.get_srv_sub().as_str(), "act": g.get_srv_act().as_str(), "channel": g.get_srv_channel(), "canRestart": g.get_srv_can_restart() },
        "claude": { "body": g.get_cc_body().as_str(), "version": g.get_cc_version().as_str(), "sub": g.get_cc_sub().as_str(), "act": g.get_cc_act().as_str(), "beyond": g.get_cc_beyond() },
        "config": { "envPath": g.get_env_path().as_str(), "projectsDir": g.get_projects_dir().as_str(), "status": g.get_config_status().as_str() },
        "prompt": { "all": g.get_sysprompt().as_str(), "project": g.get_proj_name().as_str(), "projectText": g.get_proj_sysprompt().as_str(), "status": g.get_sp_status().as_str() },
        "notify": rows(g.get_notify()),
        "channels": (0..chans.row_count()).filter_map(|i| chans.row_data(i)).map(|c| json!({ "id": c.id.as_str(), "enabled": c.enabled, "canToggle": c.can_toggle, "telegram": c.telegram })).collect::<Vec<_>>(),
        "telegramChannel": { "state": g.get_tgc_state().as_str(), "chatId": g.get_chat_id().as_str() },
        "signin": {
            "pwSet": g.get_pw_set(), "pwOn": g.get_pw_on(), "enrol": g.get_enrol_secret().as_str(),
            "tgHas": g.get_tg_has(), "tgOn": g.get_tg_on(), "tgStatus": g.get_tg_status().as_str(), "tgTokenSet": g.get_tg_token_set(), "tgUser": g.get_tg_user().as_str(),
            "oauth": g.get_oauth_has(), "google": oauth(g.get_google()), "github": oauth(g.get_github()), "googleId": g.get_google_id().as_str(),
        },
        "phone": { "push": g.get_push_state().as_str(), "line": g.get_push_line().as_str(), "contact": g.get_push_contact().as_str(),
            "pairOpen": g.get_pair_open(), "pairLine": g.get_pair_line().as_str(), "pairAgain": g.get_pair_again(), "pairImage": g.get_pair_has_image() },
        "session": { "open": g.get_sess_open(), "state": g.get_sess_state().as_str(), "others": g.get_sess_others(),
            "rows": (0..sess.row_count()).filter_map(|i| sess.row_data(i)).map(|s| json!({ "id": s.id.as_str(), "label": s.label.as_str(), "current": s.current, "desc": s.desc.as_str() })).collect::<Vec<_>>() },
        "terminal": { "newlineKey": g.get_newline_key(), "autopark": g.get_autopark().as_str(), "min": g.get_autopark_min() },
        "voice": { "shown": g.get_vs_shown(), "model": g.get_vs_model(), "models": g.get_vs_models().row_count(), "status": g.get_vs_status().as_str(), "admin": g.get_vs_admin(),
            "spend": g.get_vs_spend().as_str(), "spendNote": g.get_vs_spend_note().as_str(), "keysNote": g.get_vs_keys_note().as_str(), "away": g.get_vs_away(), "awayClasses": g.get_vs_away_classes().row_count() },
        "desktop": { "launch": g.get_launch(), "autoUpdate": g.get_auto_update(), "channel": g.get_upd_channel(), "status": g.get_upd_status().as_str(), "dot": w.global::<crate::Shell>().get_update_dot() },
        "modal": g.get_modal(),
        "opened": opened,
    })
}
