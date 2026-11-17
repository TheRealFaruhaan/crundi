//! Settings → Backup & restore (the web's renderBackupSection,
//! bkStatusHtml, drawBackupForm, drawBackupItems, onBackupClick,
//! onBackupStatus, drawBackupNotice), Settings → Usage (keep the 5-hour
//! window warm), and the sign-in screen's restore (lrStorage, lrPoll,
//! lrWaitBack).

use super::common::{self as c, ok};
use super::{get, upd, Sk};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Map, Value};

/// A line of the status box: "line" (key, bold, text, tone), "bar",
/// "waiting", "staged" (text "restart" when Crundi can restart itself),
/// "run" (tone "on" when set up).
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Line { pub kind: &'static str, pub key: String, pub bold: String, pub text: String, pub tone: String }

#[derive(Default)]
pub(crate) struct Bk {
    st: Option<Value>,
    items: Option<Result<Vec<Value>, String>>,
    confirm_key: String,
    notice_hidden: f64,
    state: String, load_error: String,
    test_out: String, test_tone: String, save_out: String, save_tone: String,
    busy: bool, notice_busy: bool, list_loading: bool,
    pub(crate) freq: i32, pub(crate) on: bool, pub(crate) style: i32, pub(crate) weekday: i32,
    tz: String, secret_ph: String, pass_label: String, pass_ph: String,
    // Usage.
    us: Option<Value>, us_state: String, us_error: String, us_on: bool,
    // The sign-in restore.
    lr_base: String, lr_items: Vec<Value>, lr_since: f64,
    lr_open: bool, lr_list: String, lr_error: String, lr_progress: bool, lr_going: bool, lr_finding: bool,
    pub(crate) lr_picked: usize, lr_phase: String, lr_frac: f32, pub(crate) lr_vhost: bool, lr_epoch: u64,
}

impl Bk {
    pub(crate) fn reset(&mut self) { self.state = "loading".into(); self.us_state = "loading".into(); self.pass_label = "Passphrase".into(); }
    pub(crate) fn signed_out(&mut self) {
        let keep = (std::mem::take(&mut self.lr_base), self.lr_open, std::mem::take(&mut self.lr_items), self.lr_epoch);
        *self = Bk::default();
        self.reset();
        (self.lr_base, self.lr_open, self.lr_items, self.lr_epoch) = keep;
    }
}

// ─── Pure rules ───

/// bkPhaseText.
pub fn phase_text(r: &Value) -> String {
    let total = c::n(r, "total");
    let pct = if total > 0.0 { format!(" {}%", (100.0 * c::n(r, "done") / total).round() as i64) } else { String::new() };
    match c::s(r, "phase").as_str() {
        "collecting" => "Gathering files…".into(),
        "packing" => format!("Packing and encrypting…{pct}"),
        "uploading" => format!("Uploading…{pct}"),
        "tidying" => "Removing old backups…".into(),
        "downloading" => format!("Downloading…{pct}"),
        "unpacking" => "Decrypting and unpacking…".into(),
        _ => "Working…".into(),
    }
}

/// bkBar: the share done, or -1 when the phase can't say.
pub fn bar_frac(r: Option<&Value>) -> f32 {
    let Some(r) = r else { return 0.0 };
    let total = c::n(r, "total");
    let ph = c::s(r, "phase");
    if total > 0.0 && ph != "unpacking" && ph != "collecting" && ph != "tidying" { (c::n(r, "done") / total).min(1.0) as f32 } else { -1.0 }
}

fn line(key: &str, bold: &str, text: &str, tone: &str) -> Line { Line { kind: "line", key: key.into(), bold: bold.into(), text: text.into(), tone: tone.into() } }
fn kind(k: &'static str, text: &str, tone: &str) -> Line { Line { kind: k, text: text.into(), tone: tone.into(), ..Default::default() } }

/// The status box (bkStatusHtml), with `when` for the dates (bkWhen).
pub fn status_lines(d: &Value, now: f64, when: &dyn Fn(f64) -> String) -> Vec<Line> {
    let mut out = Vec::new();
    let running = d.get("running").filter(|x| x.is_object());
    let waiting = d.get("waiting").filter(|x| x.is_object());
    if let Some(r) = running {
        out.push(line("", if c::s(r, "reason") == "restore" { "Restoring" } else { "Backing up" }, &phase_text(r), ""));
        out.push(kind("bar", "", ""));
    } else if waiting.is_some() {
        out.push(line("", "A scheduled backup is due.", "It starts once you have left Crundi.", ""));
        out.push(kind("waiting", "", ""));
    }
    if c::t(d, "staged") {
        out.push(line("", "A restore is ready.", "It finishes when Crundi restarts.", ""));
        out.push(kind("staged", if c::t(d, "restartMode") { "restart" } else { "" }, ""));
    }
    if let Some(a) = d.get("restoreAttempt").filter(|x| x.is_object()) {
        if !ok(a) && running.is_none() { out.push(line("Restore", &c::s(a, "error"), "", "bad")); }
    }
    let configured = c::t(d, "configured");
    if let Some(l) = d.get("last").filter(|x| x.is_object()) {
        let at = c::ms_of(l, "at").unwrap_or(0.0);
        if ok(l) {
            let how = if c::s(l, "reason") == "scheduled" { "scheduled" } else { "made by hand" };
            out.push(line("Last backup", &when(at), &format!("{}, {} files, {how}", c::fmt_bytes(c::n(l, "size")), c::s(l, "files")), "good"));
        } else {
            out.push(line("Last backup", &format!("Failed {}", when(at)), &c::s(l, "error"), "bad"));
        }
    } else if configured {
        out.push(line("Last backup", "", "None yet", ""));
    }
    match c::ms_of(d, "nextAt").filter(|x| *x > 0.0) {
        Some(n) => out.push(line("Next backup", &when(n), format!("{} time", c::s(d, "timezone")).trim(), "")),
        None => out.push(line("Next backup", "", if configured { "Scheduled backups are off" } else { "Set up the storage and a passphrase below" }, "")),
    }
    if let Some(r) = d.get("restored").filter(|x| x.is_object()) {
        let at = c::ms_of(r, "at").unwrap_or(0.0);
        if now - at < 7.0 * 86400000.0 {
            let made = c::ms_of(r, "backupCreatedAt").unwrap_or(0.0);
            let host = c::s(r, "fromHost");
            let on = if host.is_empty() { String::new() } else { format!(" on {host}") };
            out.push(line("Restored", "", &format!("{} from the backup made {}{on}. What it replaced is in {}.", when(at), when(made), c::s(r, "safetyCopy")), ""));
        }
    }
    if running.is_none() && waiting.is_none() { out.push(kind("run", "", if configured { "on" } else { "" })); }
    out
}

/// Number(x) as JSON: a number, 0 for blank, null for anything else (NaN).
pub fn number(s: &str) -> Value {
    let t = s.trim();
    if t.is_empty() { return json!(0); }
    t.parse::<f64>().ok().map(|n| if n.fract() == 0.0 { json!(n as i64) } else { json!(n) }).unwrap_or(Value::Null)
}

/// The warm-up's two lines: the last warm-up, and the window.
pub fn usage_lines(w: &Value, now: f64, fmt: &dyn Fn(f64) -> String) -> (String, String) {
    let last = c::ms_of(w, "lastWarmAt").filter(|x| *x > 0.0).map(|at| {
        let r = c::s(w, "lastWarmResult");
        format!("Last: {} · {}", if r.is_empty() { "—".into() } else { r }, fmt(at))
    }).unwrap_or_default();
    let reset = c::ms_of(w, "lastKnownReset").map(|at| if at > now { format!("Current window resets {}", fmt(at)) } else { format!("No window open · last one ended {}", fmt(at)) }).unwrap_or_default();
    (last, reset)
}

// ─── Settings → Backup & restore ───

pub fn load(cx: &mut App) {
    crundi_shell::call(cx, |api| api.get("/api/backup"), |r, cx| {
        match r {
            Ok(d) if ok(&d) => { upd(cx, |s| s.b.st = Some(d)); fill_form(cx); }
            Ok(d) => upd(cx, |s| { s.b.state = "error".into(); s.b.load_error = c::err_of(&d, "Could not load the backup settings"); }),
            Err(e) => upd(cx, |s| { s.b.state = "error".into(); s.b.load_error = e; }),
        }
    });
}

/// drawBackupForm: the form from the settings.
fn fill_form(cx: &mut App) {
    upd(cx, |s| {
        let Some(d) = s.b.st.clone() else { return };
        let st = d.get("storage").cloned().unwrap_or_default();
        let sch = d.get("schedule").cloned().unwrap_or_default();
        s.b.state = "data".into();
        for (k, n) in [("bk-endpoint", "endpoint"), ("bk-bucket", "bucket"), ("bk-region", "region"), ("bk-prefix", "prefix"), ("bk-keyid", "accessKeyId")] { s.f.set(k, c::s(&st, n)); }
        s.b.style = if st.get("pathStyle") == Some(&Value::Bool(false)) { 1 } else { 0 };
        s.f.set("bk-secret", "");
        s.b.secret_ph = if c::t(&st, "hasSecret") { "Saved. Type to replace it".into() } else { String::new() };
        s.b.test_out.clear();
        let has = c::t(&d, "hasPassphrase");
        s.b.pass_label = if has { "New passphrase" } else { "Passphrase" }.into();
        s.b.pass_ph = if has { "Set. Type to change it" } else { "At least 8 characters" }.into();
        s.f.set("bk-pass", "");
        s.f.set("bk-pass2", "");
        s.b.on = c::t(&sch, "enabled");
        s.b.freq = match c::s(&sch, "frequency").as_str() { "weekly" => 1, "hours" => 2, _ => 0 };
        s.b.weekday = c::n(&sch, "weekday") as i32;
        s.f.set("bk-hours", c::s(&sch, "everyHours"));
        s.f.set("bk-time", c::s(&sch, "time"));
        s.f.set("bk-keep", c::s(&d, "keep"));
        s.b.tz = c::s(&d, "timezone");
        s.b.save_out.clear();
        if !s.b.confirm_key.is_empty() { s.f.set("bk-restore-pass", ""); }
    });
}

fn items_state(b: &Bk) -> (&'static str, &'static str) {
    if b.list_loading { return (match &b.items { None => "", Some(Err(_)) => "error", Some(Ok(v)) if v.is_empty() => "empty", _ => "data" }, "Loading…"); }
    match &b.items { None => ("", "Show"), Some(Err(_)) => ("error", "Refresh"), Some(Ok(v)) if v.is_empty() => ("empty", "Refresh"), Some(Ok(_)) => ("data", "Refresh") }
}

fn load_items(cx: &mut App) {
    upd(cx, |s| s.b.list_loading = true);
    crundi_shell::call(cx, |api| api.get("/api/backup/list"), |r, cx| {
        let items = match r { Ok(d) if ok(&d) => Ok(d.get("items").and_then(|x| x.as_array()).cloned().unwrap_or_default()), Ok(d) => Err(c::err_of(&d, "Could not list the backups")), Err(e) => Err(e) };
        upd(cx, |s| { s.b.items = Some(items); s.b.list_loading = false; });
    });
}

fn storage(s: &Sk) -> Value {
    let region = s.f.trim("bk-region");
    json!({
        "endpoint": s.f.trim("bk-endpoint"), "bucket": s.f.trim("bk-bucket"),
        "region": if region.is_empty() { "auto".to_string() } else { region },
        "prefix": s.f.trim("bk-prefix"), "accessKeyId": s.f.trim("bk-keyid"), "secretAccessKey": s.f.trim("bk-secret"),
        "pathStyle": s.b.style != 1,
    })
}

fn bk_busy(cx: &mut App, on: bool) { upd(cx, |s| s.b.busy = on); }

pub fn act(cx: &mut App, action: &str, arg: &str) {
    let arg = arg.to_string();
    match action {
        "bk-run" | "bkn-run" => {
            let notice = action == "bkn-run";
            upd(cx, |s| if notice { s.b.notice_busy = true } else { s.b.busy = true });
            c::post(cx, "/api/backup/run", json!({}), move |d, cx| {
                if notice { upd(cx, |s| s.b.notice_busy = false); }
                if !ok(&d) { c::toast_error(cx, &c::err_of(&d, "Could not start the backup")); bk_busy(cx, false); }
                // Stays off until the status says it is running.
                else if !notice { c::after(cx, 2000, |cx| bk_busy(cx, false)); }
            });
        }
        "bk-skip" | "bkn-skip" => {
            let notice = action == "bkn-skip";
            if notice { upd(cx, |s| s.b.notice_busy = true); }
            c::post(cx, "/api/backup/skip", json!({}), move |d, cx| {
                if notice {
                    upd(cx, |s| s.b.notice_busy = false);
                    if !ok(&d) { c::toast_error(cx, &c::err_of(&d, "Could not skip it")); } else { c::toast(cx, "Skipped. The next backup runs on schedule."); }
                }
            });
        }
        "bkn-hide" => upd(cx, |s| s.b.notice_hidden = s.b.st.as_ref().and_then(|x| x.get("waiting")).map(|w| c::n(w, "slot")).unwrap_or(0.0)),
        "bk-restart" => c::post(cx, "/api/backup/restart", json!({}), |d, cx| if !ok(&d) { c::toast_error(cx, &c::err_of(&d, "Could not restart")); } else { c::toast(cx, "Restarting to finish the restore"); }),
        "bk-cancel-restore" => c::post(cx, "/api/backup/cancel-restore", json!({}), |_, cx| load(cx)),
        "bk-test" => {
            let st = upd(cx, |s| { s.b.test_out = "Checking…".into(); s.b.test_tone.clear(); storage(s) });
            c::post(cx, "/api/backup/test", json!({ "storage": st }), |d, cx| upd(cx, |s| {
                if ok(&d) { s.b.test_out = "Connected. The bucket can be listed.".into(); s.b.test_tone = "good".into(); }
                else { s.b.test_out = c::s(&d, "error"); s.b.test_tone = "bad".into(); }
            }));
        }
        "bk-save" => save(cx),
        "bk-list" => load_items(cx),
        "bk-restore-ask" => upd(cx, |s| { s.b.confirm_key = arg; s.f.set("bk-restore-pass", ""); }),
        "bk-restore-cancel" => upd(cx, |s| s.b.confirm_key.clear()),
        "bk-restore-go" => {
            bk_busy(cx, true);
            let pass = get(cx, |s| s.f.get("bk-restore-pass"));
            c::post(cx, "/api/backup/restore", json!({ "key": arg, "passphrase": pass }), |d, cx| {
                bk_busy(cx, false);
                if !ok(&d) { c::toast_error(cx, &c::err_of(&d, "Could not start the restore")); return; }
                upd(cx, |s| s.b.confirm_key.clear());
                c::toast(cx, "Restoring. Crundi restarts when the backup is unpacked.");
            });
        }
        "bk-delete" => c::confirm(cx, "Delete backup", "Delete this backup from storage? This cannot be undone.", "Delete", true, move |cx| {
            c::post(cx, "/api/backup/delete", json!({ "key": arg }), |d, cx| {
                if !ok(&d) { c::toast_error(cx, &c::err_of(&d, "Could not delete it")); return; }
                c::toast(cx, "Backup deleted");
                load_items(cx);
            });
        }),
        _ => {}
    }
}

fn save(cx: &mut App) {
    let (p1, p2) = get(cx, |s| (s.f.get("bk-pass"), s.f.get("bk-pass2")));
    if p1 != p2 { upd(cx, |s| { s.b.save_out = "The two passphrases differ".into(); s.b.save_tone = "bad".into(); }); return; }
    let body = get(cx, |s| {
        let time = s.f.trim("bk-time");
        let freq = ["daily", "weekly", "hours"][s.b.freq.clamp(0, 2) as usize];
        json!({
            "storage": storage(s), "passphrase": p1,
            "schedule": {
                "enabled": s.b.on,
                "frequency": freq,
                "time": if time.is_empty() { "03:00".to_string() } else { time },
                "weekday": s.b.weekday,
                "everyHours": number(&s.f.get("bk-hours")),
            },
            "keep": number(&s.f.get("bk-keep")),
        })
    });
    c::post(cx, "/api/backup/settings", body, |d, cx| {
        if !ok(&d) { upd(cx, |s| { s.b.save_out = c::s(&d, "error"); s.b.save_tone = "bad".into(); }); return; }
        c::toast_ok(cx, "Backup settings saved");
        if let Some(cfg) = d.get("config").and_then(|x| x.as_object()).cloned() { upd(cx, |s| merge(&mut s.b, cfg)); }
        load(cx);
    });
}

fn merge(b: &mut Bk, st: Map<String, Value>) {
    let mut o = b.st.take().and_then(|v| v.as_object().cloned()).unwrap_or_default();
    for (k, v) in st { o.insert(k, v); }
    b.st = Some(Value::Object(o));
}

/// A status push from the server ("backup"): Settings, and the notice.
pub fn on_status(cx: &mut App, st: &Value) {
    let Some(obj) = st.as_object().cloned() else { return };
    let prev = upd(cx, |s| {
        let prev = s.b.st.as_ref().and_then(|x| x.get("running")).filter(|x| x.is_object()).cloned();
        merge(&mut s.b, obj);
        prev
    });
    let running = st.get("running").filter(|x| x.is_object()).is_some();
    let last = st.get("last").filter(|x| x.is_object());
    if prev.is_some() && !running {
        bk_busy(cx, false);
        if last.is_some() && get(cx, |s| s.b.items.is_some()) { load_items(cx); }
    }
    if let (Some(p), false, Some(l)) = (&prev, running, last) {
        if c::s(p, "reason") != "restore" {
            if ok(l) { c::toast_ok(cx, "Backup uploaded"); } else { c::toast_error(cx, &format!("Backup failed: {}", c::s(l, "error"))); }
        }
    }
    if c::t(st, "restarting") { c::toast(cx, "Restore ready. Crundi is restarting…"); }
    if let (Some(a), Some(p)) = (st.get("restoreAttempt").filter(|x| x.is_object()), &prev) {
        if !ok(a) && c::s(p, "reason") == "restore" { c::toast_error(cx, &format!("Restore failed: {}", c::s(a, "error"))); }
    }
}

fn notice_visible(b: &Bk) -> bool {
    let w = b.st.as_ref().and_then(|s| s.get("waiting")).filter(|x| x.is_object());
    w.is_some_and(|w| b.notice_hidden != c::n(w, "slot"))
}

/// refreshBackupNotice: on sign-in and on every reconnect (the owner's).
pub fn refresh_notice(cx: &mut App) {
    crundi_shell::call(cx, |api| api.get("/api/backup"), |r, cx| {
        if let Ok(d) = r { if ok(&d) { if let Some(o) = d.as_object().cloned() { upd(cx, |s| merge(&mut s.b, o)); } } }
    });
}

// ─── Settings → Usage ───

pub fn usage_load(cx: &mut App) {
    crundi_shell::call(cx, |api| api.get("/api/settings"), |r, cx| upd(cx, |s| match r {
        Ok(d) if ok(&d) => {
            let w = d.get("limitWarmup").cloned().unwrap_or(json!({ "enabled": false }));
            s.b.us_on = c::t(&w, "enabled");
            s.b.us = Some(w);
            s.b.us_state = "data".into();
        }
        Ok(d) => { s.b.us_state = "error".into(); s.b.us_error = c::err_of(&d, "Could not load the settings"); }
        Err(e) => { s.b.us_state = "error".into(); s.b.us_error = e; }
    }));
}

fn usage_set(cx: &mut App, on: bool) {
    upd(cx, |s| s.b.us_on = on);
    c::post(cx, "/api/settings", json!({ "limitWarmup": on }), move |d, cx| {
        if !ok(&d) {
            upd(cx, |s| s.b.us_on = !on);
            c::toast_error(cx, &format!("Could not change that: {}", c::err_of(&d, "Failed")));
            return;
        }
        upd(cx, |s| if let Some(u) = &mut s.b.us { u["enabled"] = json!(on); });
        let gone = c::n(&d, "clearedLimitReset") as i64;
        if on { c::toast_ok(cx, "Crundi will keep the 5-hour window warm"); }
        else if gone > 0 { c::toast_ok(cx, &format!("Window warm-up off - cleared {gone}{}", if gone == 1 { " queued message" } else { " queued messages" })); }
        else { c::toast_ok(cx, "Window warm-up off"); }
    });
}

/// The switch: on at once; off asks first when messages wait on the reset.
pub fn usage_toggle(cx: &mut App, on: bool) {
    if on { usage_set(cx, true); return; }
    crundi_shell::call(cx, |api| api.get("/api/settings"), |r, cx| {
        let queued = r.ok().map(|d| c::n(&d, "queuedLimitReset") as i64).unwrap_or(0);
        if queued <= 0 { usage_set(cx, false); return; }
        let subject = if queued == 1 { "1 scheduled message is".to_string() } else { format!("{queued} scheduled messages are") };
        let msg = format!("{subject} waiting to send when the limit resets. Warm-up is what notices the reset, so turning it off clears {}.", if queued == 1 { "it" } else { "them" });
        // Until the answer the switch shows what it is.
        upd(cx, |s| s.b.us_on = true);
        c::confirm(cx, "Turn off window warm-up?", &msg, "Turn off", true, |cx| usage_set(cx, false));
    });
}

// ─── Restore at sign-in ───

/// Open "Restore from a backup" over the sign-in page, for `server` (its base URL).
pub fn open_login_restore(cx: &mut App, server: &str) {
    let base = server.trim_end_matches('/').to_string();
    upd(cx, |s| {
        let b = &mut s.b;
        b.lr_base = base; b.lr_items.clear(); b.lr_list.clear(); b.lr_error.clear(); b.lr_progress = false; b.lr_going = false;
        b.lr_finding = false; b.lr_picked = 0; b.lr_open = true; b.lr_epoch += 1;
        s.f.set("lr-pass", "");
    });
}

fn lr_storage(s: &Sk) -> Value {
    let region = s.f.trim("lr-region");
    json!({
        "endpoint": s.f.trim("lr-endpoint"), "bucket": s.f.trim("lr-bucket"),
        "region": if region.is_empty() { "auto".to_string() } else { region },
        "prefix": s.f.trim("lr-prefix"), "accessKeyId": s.f.trim("lr-key"), "secretAccessKey": s.f.trim("lr-secret"),
        "pathStyle": !s.b.lr_vhost,
    })
}

/// An unauthenticated call to the server being restored, off the UI thread.
fn open(cx: &mut App, method: &'static str, path: &'static str, body: Option<Value>, done: impl FnOnce(Result<Value, String>, &mut App) + 'static) {
    let base = get(cx, |s| s.b.lr_base.clone());
    crundi_shell::run(cx, move || crundi_core::api::open_call(&base, method, path, body.as_ref()).map_err(|e| match e {
        crundi_core::api::ApiError::Net(m) => format!("Could not reach the server: {m}"),
        e => e.to_string(),
    }), done);
}

pub fn lr_act(cx: &mut App, action: &str) {
    match action {
        "lr-back" => upd(cx, |s| { s.b.lr_open = false; s.b.lr_epoch += 1; }),
        "lr-find" => {
            let st = upd(cx, |s| { s.b.lr_error.clear(); s.b.lr_finding = true; lr_storage(s) });
            open(cx, "POST", "/api/backup/setup/list", Some(json!({ "storage": st })), |r, cx| upd(cx, |s| {
                s.b.lr_finding = false;
                let d = match r { Ok(d) => d, Err(e) => { s.b.lr_error = e; return; } };
                if !ok(&d) { s.b.lr_error = c::err_of(&d, "Could not list the bucket"); return; }
                let items = d.get("items").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                s.b.lr_list = if items.is_empty() { "empty" } else { "data" }.into();
                s.b.lr_items = items;
                s.b.lr_picked = 0;
            }));
        }
        "lr-go" => {
            let key = get(cx, |s| s.b.lr_items.get(s.b.lr_picked).map(|x| c::s(x, "key")));
            let Some(key) = key else { upd(cx, |s| s.b.lr_error = "Choose a backup".into()); return };
            let pass = get(cx, |s| s.f.get("lr-pass"));
            if pass.is_empty() { upd(cx, |s| s.b.lr_error = "Enter the passphrase the backup was made with".into()); return; }
            let (st, epoch) = upd(cx, |s| { s.b.lr_error.clear(); s.b.lr_going = true; (lr_storage(s), s.b.lr_epoch) });
            open(cx, "POST", "/api/backup/setup/restore", Some(json!({ "storage": st, "key": key, "passphrase": pass })), move |r, cx| {
                let d = match r { Ok(d) => d, Err(e) => { upd(cx, |s| { s.b.lr_error = e; s.b.lr_going = false; }); return; } };
                if !ok(&d) { upd(cx, |s| { s.b.lr_error = c::err_of(&d, "Could not start the restore"); s.b.lr_going = false; }); return; }
                upd(cx, |s| { s.b.lr_progress = true; s.b.lr_phase.clear(); s.b.lr_frac = -1.0; });
                lr_poll(cx, c::t(&d, "restartMode"), epoch);
            });
        }
        _ => {}
    }
}

fn lr_live(cx: &mut App, epoch: u64) -> bool { get(cx, |s| s.b.lr_open && s.b.lr_epoch == epoch) }

fn lr_poll(cx: &mut App, mode: bool, epoch: u64) {
    open(cx, "GET", "/api/backup/setup/status", None, move |r, cx| {
        if !lr_live(cx, epoch) { return; }
        let d = r.ok();
        if let Some(run) = d.as_ref().and_then(|d| d.get("running")).filter(|x| x.is_object()) {
            let (p, f) = (phase_text(run), bar_frac(Some(run)));
            upd(cx, |s| { s.b.lr_phase = p; s.b.lr_frac = f; });
            c::after(cx, 800, move |cx| lr_poll(cx, mode, epoch));
            return;
        }
        if let Some(a) = d.as_ref().and_then(|d| d.get("restoreAttempt")).filter(|x| x.is_object()) {
            if !ok(a) { let e = c::s(a, "error"); upd(cx, |s| { s.b.lr_progress = false; s.b.lr_error = e; s.b.lr_going = false; }); return; }
            if !mode { upd(cx, |s| { s.b.lr_phase = "The backup is ready. Restart Crundi to finish restoring it.".into(); s.b.lr_frac = 0.0; }); return; }
            upd(cx, |s| { s.b.lr_phase = "Restored. Crundi is restarting; sign in with the backup’s password when it is back.".into(); s.b.lr_frac = 0.0; s.b.lr_since = c::now_ms(); });
            lr_wait_back(cx, epoch);
            return;
        }
        // Not started yet, or the server is already restarting.
        c::after(cx, 800, move |cx| lr_poll(cx, mode, epoch));
    });
}

fn lr_wait_back(cx: &mut App, epoch: u64) {
    c::after(cx, 2500, move |cx| {
        if !lr_live(cx, epoch) { return; }
        open(cx, "GET", "/api/auth/methods", None, move |r, cx| {
            if let Ok(m) = r { if ok(&m) && !c::t(&m, "setupRequired") {
                // Back: the sign-in page asks the server again (the web reloads the page).
                let base = upd(cx, |s| { s.b.lr_open = false; s.b.lr_base.clone() });
                crundi_shell::session::check_server(cx, &base);
                return;
            } }
            if c::now_ms() - get(cx, |s| s.b.lr_since) < 180000.0 { lr_wait_back(cx, epoch); }
            else { upd(cx, |s| s.b.lr_phase = "Crundi has not come back yet. Reload this page in a moment.".into()); }
        });
    });
}

// ─── Drawing ───

fn hint(text: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_12).text_color(T::text_muted()).child(text.into()) }
fn small(text: impl Into<SharedString>, tone: &str) -> Div {
    div().text_size(T::TEXT_11).text_color(match tone { "good" => T::green(), "bad" => T::red(), _ => T::text_muted() }).child(text.into())
}
fn head(text: &str) -> Div { div().text_size(T::TEXT_11).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(text.to_string()) }
fn lbl(text: impl Into<SharedString>) -> Div { div().text_size(T::TEXT_12).text_color(T::text_secondary()).child(text.into()) }
fn row() -> Div { div().flex().items_center().gap(T::SP_8) }
fn col(gap: Pixels) -> Div { div().flex().flex_col().gap(gap) }
fn bar(frac: f32) -> Div {
    // A phase that can't say how far it is: the whole track, dimmed (GPUI: no sliding fill here).
    if frac < 0.0 { w::meter(1.0, T::accent_dim(), T::SP_6) } else { w::meter(frac, T::accent(), T::SP_6) }
}
fn field(label: impl Into<SharedString>, input: AnyElement, hint_text: &str) -> Div {
    col(T::SP_4).flex_1().min_w_0().child(lbl(label)).child(input).when(!hint_text.is_empty(), |d| d.child(hint(hint_text.to_string())))
}
fn btn(id: impl Into<ElementId>, label: &str, icon: Option<IconName>, primary: bool, off: bool, f: impl Fn(&mut App) + 'static) -> AnyElement {
    let b = if primary { w::primary(id, label.to_string()) } else { w::button(id, label.to_string()) };
    let b = if let Some(i) = icon { b.icon(i) } else { b };
    w::disabled(b, off).on_click(move |_, _, cx| f(cx)).into_any_element()
}

fn restore_key(k: &'static str, arg: &str, cx: &mut App) {
    if c::key_arg(arg).1 { return; }
    match k {
        "bk-restore-pass" => { let key = get(cx, |s| s.b.confirm_key.clone()); act(cx, "bk-restore-go", &key); }
        "lr-pass" => lr_act(cx, "lr-go"),
        _ => {}
    }
}

pub fn render(window: &mut Window, cx: &mut App) -> AnyElement {
    let state = get(cx, |s| s.b.state.clone());
    if state == "loading" { return hint("Loading…").into_any_element(); }
    if state == "error" { return small(get(cx, |s| s.b.load_error.clone()), "bad").text_size(T::TEXT_12).into_any_element(); }
    let (secret_ph, pass_ph, confirming) = get(cx, |s| (s.b.secret_ph.clone(), s.b.pass_ph.clone(), !s.b.confirm_key.is_empty()));
    let ln = |window: &mut Window, cx: &mut App, k: &'static str, ph: &str, masked: bool| c::line::<Sk>(window, cx, k, ph, masked, None);
    let ep = ln(window, cx, "bk-endpoint", "https://ACCOUNT.r2.cloudflarestorage.com", false);
    let bu = ln(window, cx, "bk-bucket", "", false);
    let rg = ln(window, cx, "bk-region", "auto", false);
    let pf = ln(window, cx, "bk-prefix", "crundi/", false);
    let ki = ln(window, cx, "bk-keyid", "", false);
    let se = ln(window, cx, "bk-secret", &secret_ph, true);
    let p1 = ln(window, cx, "bk-pass", &pass_ph, true);
    let p2 = ln(window, cx, "bk-pass2", "", true);
    let hr = ln(window, cx, "bk-hours", "", false);
    let tm = ln(window, cx, "bk-time", "03:00", false);
    let kp = ln(window, cx, "bk-keep", "", false);
    let rp = confirming.then(|| c::line::<Sk>(window, cx, "bk-restore-pass", "Passphrase (blank: the saved one)", true, Some(restore_key)));
    let s = cx.global::<Sk>();
    let b = &s.b;
    let d = b.st.clone().unwrap_or_default();
    let frac = bar_frac(d.get("running").filter(|x| x.is_object()));
    let mut status = col(T::SP_6).px(T::SP_16).py(T::SP_12).rounded(T::R_FIELD).border_1().border_color(T::border()).bg(T::bg());
    for (i, l) in status_lines(&d, c::now_ms(), &c::when).into_iter().enumerate() {
        status = status.child(match l.kind {
            "line" => div().flex().gap(T::SP_10).text_size(T::TEXT_12)
                .when(!l.key.is_empty(), |d| d.child(div().flex_none().w(T::SKILLS_KEY_W).text_color(T::text_muted()).child(l.key.clone())))
                .when(!l.bold.is_empty(), |d| d.child(div().flex_none().font_weight(T::W_SEMIBOLD).text_color(match l.tone.as_str() { "bad" => T::red(), "good" => T::green(), _ => T::text() }).child(l.bold.clone())))
                .child(div().flex_1().min_w_0().text_color(T::text_secondary()).child(l.text.clone())).into_any_element(),
            "bar" => bar(frac).into_any_element(),
            "waiting" => row().pt(T::SP_4).child(btn(("bk-run", i), "Back up now", None, true, b.busy, |cx| act(cx, "bk-run", "")))
                .child(btn(("bk-skip", i), "Skip this one", None, false, false, |cx| act(cx, "bk-skip", ""))).into_any_element(),
            "staged" => row().pt(T::SP_4)
                .when(l.text == "restart", |d| d.child(btn(("bk-restart", i), "Restart now", None, true, false, |cx| act(cx, "bk-restart", ""))))
                .when(l.text != "restart", |d| d.child(hint("Restart Crundi the way you started it.")))
                .child(btn(("bk-cancel", i), "Cancel the restore", None, false, false, |cx| act(cx, "bk-cancel-restore", ""))).into_any_element(),
            _ => row().pt(T::SP_4).child(btn(("bk-run", i), "Back up now", Some(IconName::Upload), true, l.tone != "on" || b.busy, |cx| act(cx, "bk-run", ""))).into_any_element(),
        });
    }
    let (items_st, list_btn) = items_state(b);
    let days = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    let mut sched = row().gap(T::SP_10)
        .child(w::segmented("bk-freq", &["Daily", "Weekly", "Every few hours"], b.freq.clamp(0, 2) as usize, |i, _, cx| upd(cx, |s| s.b.freq = *i as i32)));
    if b.freq == 1 {
        sched = sched.child(w::select("bk-weekday", days.iter().map(|d| SharedString::from(*d)).collect(), Some(b.weekday.clamp(0, 6) as usize), "", false, |i, _, cx| upd(cx, |s| s.b.weekday = i as i32)));
    }
    if b.freq == 2 { sched = sched.child(lbl("every")).child(div().w(T::SP_48 + T::SP_24).child(w::input(&hr))).child(lbl("hours from")); } else { sched = sched.child(lbl("at")); }
    sched = sched.child(div().w(T::SP_48 + T::SP_32).font_family(T::MONO).child(w::input(&tm)))
        .child(small(if b.tz.is_empty() { String::new() } else { format!("{} time", b.tz) }, ""));
    let mut items = col(T::SP_10).pt(T::SP_2)
        .child(row().child(head("Backups in storage").flex_1().min_w_0()).child(btn("bk-list", list_btn, None, false, false, |cx| act(cx, "bk-list", ""))));
    match (items_st, &b.items) {
        ("error", Some(Err(e))) => items = items.child(small(e.clone(), "bad").text_size(T::TEXT_12)),
        ("empty", _) => items = items.child(div().p(T::SP_12).rounded(T::R_FIELD).border_1().border_color(T::border()).text_size(T::TEXT_13).text_color(T::text_muted())
            .child("No backups in this folder yet. Back up now to make the first.")),
        ("data", Some(Ok(v))) => {
            let mut list = div().flex().flex_col().rounded(T::R_FIELD).border_1().border_color(T::border()).overflow_hidden();
            for (i, it) in v.iter().enumerate() {
                let key = c::s(it, "key");
                let (k1, k2, k3) = (key.clone(), key.clone(), key.clone());
                let confirming = key == b.confirm_key;
                let host = c::s(it, "host");
                list = list.child(col(T::SP_4).px(T::SP_12).py(T::SP_10).when(i > 0, |d| d.border_t_1().border_color(T::border_subtle()))
                    .child(row().gap(T::SP_6)
                        .child(col(T::SP_4).flex_1().min_w_0()
                            .child(div().text_size(T::TEXT_13).text_color(T::text()).child(c::ms_of(it, "at").map(c::medium_short).unwrap_or_default()))
                            .child(row().gap(T::SP_10).child(w::tag(if c::s(it, "reason") == "scheduled" { "Scheduled" } else { "By hand" }, ""))
                                .child(small(c::fmt_bytes(c::n(it, "size")), "")).when(!host.is_empty(), |d| d.child(small(host.clone(), "")))))
                        .child(btn(("bk-restore-ask", i), "Restore", Some(IconName::Download), false, false, move |cx| act(cx, "bk-restore-ask", &k1)))
                        .child(w::icon_button(("bk-delete", i), IconName::Trash, "Delete").on_click(move |_, _, cx| act(cx, "bk-delete", &k2))))
                    .when(confirming, |d| d.child(col(T::SP_8).p(T::SP_12).rounded(T::R_FIELD).border_1().border_color(T::yellow()).bg(T::yellow_dim())
                        .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("This replaces this Crundi’s settings, sign-in, projects, layouts, kanban, notes, mindmap, media, schedules and services with the backup’s, and adds back its Claude transcripts and any global skills that are not already here. What it replaces is kept in a pre-restore folder. Crundi restarts to finish."))
                        .child(row().gap(T::SP_10).children(rp.as_ref().map(|e| div().flex_1().min_w_0().child(w::input(e))))
                            .child(btn(("bk-restore-go", i), "Restore and restart", None, true, b.busy, move |cx| act(cx, "bk-restore-go", &k3)))
                            .child(btn(("bk-restore-cancel", i), "Cancel", None, false, false, |cx| act(cx, "bk-restore-cancel", "")))))));
            }
            items = items.child(list);
        }
        _ => {}
    }
    col(T::SP_16)
        .child(status)
        .child(col(T::SP_10).child(head("Storage"))
            .child(field("Endpoint", w::input(&ep).into_any_element(), "Any S3-compatible storage: Cloudflare R2, AWS S3, Backblaze B2, MinIO, Wasabi…"))
            .child(div().flex().gap(T::SP_16).child(field("Bucket", w::input(&bu).into_any_element(), "")).child(field("Region", w::input(&rg).into_any_element(), "R2 uses auto; AWS needs the bucket’s region.")))
            .child(div().flex().gap(T::SP_16).child(field("Folder", w::input(&pf).into_any_element(), ""))
                .child(field("Addresses", div().flex().child(w::segmented("bk-style", &["Path", "Virtual-hosted"], b.style.clamp(0, 1) as usize, |i, _, cx| upd(cx, |s| s.b.style = *i as i32))).into_any_element(), "")))
            .child(div().flex().gap(T::SP_16).child(field("Access key ID", w::input(&ki).into_any_element(), "")).child(field("Secret access key", w::input(&se).into_any_element(), "")))
            .child(row().child(btn("bk-test", "Test the connection", None, false, false, |cx| act(cx, "bk-test", ""))).child(small(b.test_out.clone(), &b.test_tone))))
        .child(col(T::SP_10).child(head("Passphrase"))
            .child(div().flex().gap(T::SP_16).child(field(b.pass_label.clone(), w::input(&p1).into_any_element(), "")).child(field("Again", w::input(&p2).into_any_element(), "")))
            .child(hint("Backups hold your sign-in and settings, so they are encrypted with this. You need it to restore; without it a backup cannot be opened. Keep it somewhere other than this server.")))
        .child(col(T::SP_10).child(head("Schedule"))
            .child(w::check("bk-on", "Back up automatically", b.on, |v, _, cx| upd(cx, |s| s.b.on = *v)))
            .child(sched)
            .child(col(T::SP_4).pt(T::SP_2).child(lbl("Keep the last"))
                .child(row().gap(T::SP_10).child(div().w(T::SP_48 + T::SP_40).child(w::input(&kp))).child(lbl("scheduled backups")))
                .child(hint("Older scheduled backups of this machine are deleted after each new one. 0 keeps them all. Backups you make by hand are kept until you delete them.")))
            .child(hint("If you are using Crundi when a backup is due, it tells you and waits until you have left.")))
        .child(row().child(btn("bk-save", "Save", None, true, false, |cx| act(cx, "bk-save", ""))).child(small(b.save_out.clone(), &b.save_tone)))
        .child(items)
        .into_any_element()
}

pub fn usage_render(_window: &mut Window, cx: &mut App) -> AnyElement {
    let s = cx.global::<Sk>();
    let b = &s.b;
    match b.us_state.as_str() {
        "loading" => return hint("Loading…").into_any_element(),
        "error" => return small(b.us_error.clone(), "bad").text_size(T::TEXT_12).into_any_element(),
        _ => {}
    }
    let (last, reset) = b.us.as_ref().map(|w| usage_lines(w, c::now_ms(), &c::locale)).unwrap_or_default();
    div().flex().gap(T::SP_16)
        .child(col(T::SP_4).flex_1().min_w_0()
            .child(div().text_size(T::TEXT_14).text_color(T::text()).child("Keep the 5-hour window warm"))
            .child(hint("Claude’s 5-hour allowance is a rolling window that starts on your first message — so beginning real work also starts the clock, and it can run out mid-session. When on, Crundi checks about once a minute and, if the window has lapsed while you were idle, sends a one-word message to open the next one. Never while a session is working, and never if your own work already opened a window."))
            .when(!last.is_empty(), |d| d.child(hint(last.clone())))
            .when(!reset.is_empty(), |d| d.child(hint(reset.clone()))))
        .child(div().flex_none().child(w::switch("us-toggle", b.us_on, |v, _, cx| usage_toggle(cx, *v))))
        .into_any_element()
}

/// The scheduled backup's notice (drawBackupNotice), bottom left.
pub fn notice_render(_window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let s = cx.global::<Sk>();
    if !notice_visible(&s.b) { return None; }
    let busy = s.b.notice_busy;
    Some(div().absolute().left(T::SP_16).bottom(T::SP_16).w(T::DIALOG_W).occlude()
        .bg(T::bg_secondary()).rounded(T::R_CARD).border_1().border_color(T::border()).border_l_2().shadow_lg()
        .pl(T::SP_16).pr(T::SP_32).py(T::SP_12).flex().flex_col().gap(T::SP_2)
        .child(div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Scheduled backup waiting"))
        .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).child("It starts once you have left Crundi, so it does not slow down your work."))
        .child(row().pt(T::SP_10)
            .child(btn("bkn-run", "Back up now", None, true, busy, |cx| act(cx, "bkn-run", "")))
            .child(btn("bkn-skip", "Skip this one", None, false, busy, |cx| act(cx, "bkn-skip", ""))))
        .child(div().absolute().top(T::SP_6).right(T::SP_6).child(w::icon_button("bkn-hide", IconName::X, "Hide").on_click(|_, _, cx| act(cx, "bkn-hide", ""))))
        .into_any_element())
}

/// "Restore from a backup" (OpsLoginRestore), over the whole window.
pub fn lr_render(window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    if !get(cx, |s| s.b.lr_open) { return None; }
    let ln = |window: &mut Window, cx: &mut App, k: &'static str, ph: &str, masked: bool| c::line::<Sk>(window, cx, k, ph, masked, Some(restore_key));
    let ep = ln(window, cx, "lr-endpoint", "Endpoint, e.g. https://ACCOUNT.r2.cloudflarestorage.com", false);
    let bu = ln(window, cx, "lr-bucket", "Bucket", false);
    let rg = ln(window, cx, "lr-region", "Region (auto)", false);
    let pf = ln(window, cx, "lr-prefix", "Folder (crundi/)", false);
    let ky = ln(window, cx, "lr-key", "Access key ID", false);
    let se = ln(window, cx, "lr-secret", "Secret access key", true);
    let ps = ln(window, cx, "lr-pass", "Backup passphrase", true);
    let s = cx.global::<Sk>();
    let b = &s.b;
    let vp = window.viewport_size();
    let mut card = col(T::SP_8).w(T::SKILLS_RESTORE_W + T::SP_48).max_h(vp.height - T::SP_48).p(T::SP_24)
        .bg(T::bg_secondary()).rounded(T::R_CARD).border_1().border_color(T::border()).shadow_lg()
        .child(div().flex().justify_center().text_size(T::TEXT_16).font_weight(T::W_SEMIBOLD).text_color(T::text()).child("Restore from a backup"))
        .child(w::input(&ep))
        .child(row().child(div().flex_1().min_w_0().child(w::input(&bu))).child(div().w(T::SP_48 + T::SP_48 + T::SP_24).child(w::input(&rg))))
        .child(w::input(&pf)).child(w::input(&ky)).child(w::input(&se))
        .child(w::check("lr-vhost", "Virtual-hosted addresses (bucket in the hostname)", b.lr_vhost, |v, _, cx| upd(cx, |s| s.b.lr_vhost = *v)))
        .child(div().flex().justify_center().child(btn("lr-find", if b.lr_finding { "Looking…" } else { "Find backups" }, None, true, b.lr_finding, |cx| lr_act(cx, "lr-find"))));
    if b.lr_list == "empty" {
        card = card.child(div().p(T::SP_12).rounded(T::R_FIELD).border_1().border_color(T::border()).text_size(T::TEXT_12).text_color(T::text_muted()).child("No backups in this folder. Check the folder name."));
    }
    if b.lr_list == "data" {
        let mut list = div().flex().flex_col().rounded(T::R_FIELD).border_1().border_color(T::border()).overflow_hidden();
        for (i, it) in b.lr_items.iter().enumerate() {
            let host = c::s(it, "host");
            list = list.child(div().id(("lr-item", i)).flex().items_center().gap(T::SP_8).h(T::CONTROL).px(T::SP_10).cursor_pointer()
                .when(i > 0, |d| d.border_t_1().border_color(T::border())).hover(|d| d.bg(T::bg_hover()))
                .child(w::choice_mark(false, i == b.lr_picked, false))
                .child(div().text_size(T::TEXT_12).text_color(T::text()).child(c::ms_of(it, "at").map(c::medium_short).unwrap_or_default()))
                .when(!host.is_empty(), |d| d.child(small(host.clone(), "")))
                .child(div().flex_1().min_w_0())
                .child(small(c::fmt_bytes(c::n(it, "size")), ""))
                .on_click(move |_, _, cx| upd(cx, |s| s.b.lr_picked = i)));
        }
        card = card.child(list).child(w::input(&ps))
            .child(div().flex().justify_center().child(btn("lr-go", "Restore", None, true, b.lr_going, |cx| lr_act(cx, "lr-go"))));
    }
    if b.lr_progress {
        card = card.child(col(T::SP_4).child(lbl(b.lr_phase.clone())).child(bar(b.lr_frac)));
    }
    card = card.child(div().min_h(T::SP_16).text_size(T::TEXT_12).text_color(T::red()).child(b.lr_error.clone()))
        .child(div().flex().justify_center().child(w::link("lr-back", "Set up a new password instead").on_click(|_, _, cx| lr_act(cx, "lr-back"))));
    Some(deferred(anchored().position(point(px(0.), px(0.))).child(
        div().id("lr-scrim").occlude().w(vp.width).h(vp.height).bg(T::scrim()).flex().items_center().justify_center()
            .child(div().id("lr-card").overflow_y_scroll().child(card)))).with_priority(1).into_any_element())
}

// ─── The driver ───

pub fn props(cx: &mut App) -> Value {
    let s = cx.global::<Sk>();
    let b = &s.b;
    let d = b.st.clone().unwrap_or_default();
    let lines: Vec<Value> = if b.st.is_some() { status_lines(&d, c::now_ms(), &c::when).iter().map(|l| json!({ "kind": l.kind, "key": l.key, "bold": l.bold, "text": l.text })).collect() } else { vec![] };
    let (items, list_btn) = items_state(b);
    let (last, reset) = b.us.as_ref().map(|w| usage_lines(w, c::now_ms(), &c::locale)).unwrap_or_default();
    json!({
        "backup": {
            "state": b.state, "error": b.load_error, "lines": lines,
            "items": items, "itemsError": match &b.items { Some(Err(e)) => e.clone(), _ => String::new() }, "listBtn": list_btn,
            "testOut": b.test_out, "testTone": b.test_tone, "saveOut": b.save_out, "notice": notice_visible(b),
            "endpoint": s.f.get("bk-endpoint"), "freq": b.freq, "keep": s.f.get("bk-keep"), "passLabel": b.pass_label,
        },
        "usage": { "state": b.us_state, "on": b.us_on, "last": last, "reset": reset },
        "restore": { "open": b.lr_open, "list": b.lr_list, "error": b.lr_error, "finding": b.lr_finding, "progress": b.lr_progress, "phase": b.lr_phase },
    })
}

#[cfg(test)]
mod tests {
    use super::{phase_text, bar_frac, status_lines, number, usage_lines};
    use serde_json::{json, Value};

    fn w(ms: f64) -> String { format!("<{}>", ms as i64) }

    #[::core::prelude::v1::test]
    fn phases() {
        assert_eq!(phase_text(&json!({ "phase": "uploading", "done": 1, "total": 4 })), "Uploading… 25%");
        assert_eq!(phase_text(&json!({ "phase": "collecting" })), "Gathering files…");
        assert_eq!(phase_text(&json!({})), "Working…");
        assert_eq!(bar_frac(Some(&json!({ "phase": "uploading", "done": 1, "total": 4 }))), 0.25);
        assert_eq!(bar_frac(Some(&json!({ "phase": "unpacking", "done": 1, "total": 4 }))), -1.0);
    }

    #[::core::prelude::v1::test]
    fn status_unconfigured() {
        let l = status_lines(&json!({ "configured": false }), 0.0, &w);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].text, "Set up the storage and a passphrase below");
        assert_eq!(l[1].kind, "run");
        assert_eq!(l[1].tone, "");
    }

    #[::core::prelude::v1::test]
    fn status_configured_running_and_staged() {
        let d = json!({ "configured": true, "running": { "reason": "manual", "phase": "packing", "done": 1, "total": 2 },
            "staged": true, "restartMode": "systemd", "nextAt": 5000, "timezone": "UTC",
            "last": { "ok": true, "at": 1000, "size": 2048, "files": 12, "reason": "scheduled" } });
        let l = status_lines(&d, 0.0, &w);
        let kinds: Vec<&str> = l.iter().map(|x| x.kind).collect();
        assert_eq!(kinds, ["line", "bar", "line", "staged", "line", "line"]);
        assert_eq!(l[0].bold, "Backing up");
        assert_eq!(l[0].text, "Packing and encrypting… 50%");
        assert_eq!(l[3].text, "restart");
        assert_eq!(l[4].text, "2.0 KB, 12 files, scheduled");
        assert_eq!(l[5].bold, "<5000>");
        assert_eq!(l[5].text, "UTC time");
    }

    #[::core::prelude::v1::test]
    fn status_waiting_and_failed() {
        let d = json!({ "configured": true, "waiting": { "slot": 3 }, "last": { "ok": false, "at": 1000, "error": "boom" }, "restoreAttempt": { "ok": false, "error": "bad pass" } });
        let l = status_lines(&d, 0.0, &w);
        assert_eq!(l[1].kind, "waiting");
        assert_eq!(l[2].key, "Restore");
        assert_eq!(l[2].bold, "bad pass");
        assert_eq!(l[3].bold, "Failed <1000>");
        assert!(l.iter().all(|x| x.kind != "run"));
    }

    #[::core::prelude::v1::test]
    fn numbers_and_warmup() {
        assert_eq!(number("12"), json!(12));
        assert_eq!(number(""), json!(0));
        assert_eq!(number("x"), Value::Null);
        let (a, b) = usage_lines(&json!({ "lastWarmAt": 1000, "lastWarmResult": "ok", "lastKnownReset": 9000 }), 5000.0, &w);
        assert_eq!(a, "Last: ok · <1000>");
        assert_eq!(b, "Current window resets <9000>");
        let (_, b) = usage_lines(&json!({ "lastKnownReset": 1000 }), 5000.0, &w);
        assert_eq!(b, "No window open · last one ended <1000>");
    }
}
