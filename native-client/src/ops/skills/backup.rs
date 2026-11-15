//! Settings → Backup & restore (the web's renderBackupSection,
//! bkStatusHtml, drawBackupForm, drawBackupItems, onBackupClick,
//! onBackupStatus, drawBackupNotice), Settings → Usage (keep the 5-hour
//! window warm), and the sign-in screen's restore (lrStorage, lrPoll,
//! lrWaitBack).

use super::common::{self as c, ok};
use crate::kit;
use crate::{OpsBkItem, OpsBkLine, OpsLrItem, OpsSkills};
use serde_json::{json, Map, Value};
use slint::ComponentHandle;
use std::{cell::RefCell, rc::Rc, time::Duration};
use slint::VecModel;

#[derive(Default)]
struct Bk {
    st: Option<Value>,
    items: Option<Result<Vec<Value>, String>>,
    confirm_key: String,
    notice_hidden: f64,
    lr_base: String,
    lr_items: Vec<Value>,
    lr_since: f64,
    us: Option<Value>,
}

thread_local! {
    static BK: RefCell<Bk> = RefCell::new(Bk::default());
    static LINES: Rc<VecModel<OpsBkLine>> = kit::model();
    static ITEMS: Rc<VecModel<OpsBkItem>> = kit::model();
    static LR: Rc<VecModel<OpsLrItem>> = kit::model();
}

fn bk<R>(f: impl FnOnce(&mut Bk) -> R) -> R { BK.with(|s| f(&mut s.borrow_mut())) }
fn g(f: impl FnOnce(&OpsSkills)) { if let Some(w) = kit::ui() { f(&w.global::<OpsSkills>()); } }
fn get(f: impl FnOnce(&OpsSkills) -> slint::SharedString) -> String { kit::ui().map(|w| f(&w.global::<OpsSkills>()).to_string()).unwrap_or_default() }

pub fn init(w: &crate::MainWindow) {
    let gl = w.global::<OpsSkills>();
    gl.set_bk_lines(LINES.with(kit::rc));
    gl.set_bk_items(ITEMS.with(kit::rc));
    gl.set_lr_items(LR.with(kit::rc));
}

pub fn signed_out() {
    let lr = bk(|b| std::mem::take(&mut b.lr_base));
    bk(|b| *b = Bk { lr_base: lr, ..Default::default() });
    g(|g| { g.set_bk_state("loading".into()); g.set_bkn_visible(false); g.set_us_state("loading".into()); });
}

// ─── Pure rules ───

/// bkPhaseText.
pub fn phase_text(r: &Value) -> String {
    let total = kit::n(r, "total");
    let pct = if total > 0.0 { format!(" {}%", (100.0 * kit::n(r, "done") / total).round() as i64) } else { String::new() };
    match kit::s(r, "phase").as_str() {
        "collecting" => "Gathering files…".into(),
        "packing" => format!("Packing and encrypting…{pct}"),
        "uploading" => format!("Uploading…{pct}"),
        "tidying" => "Removing old backups…".into(),
        "downloading" => format!("Downloading…{pct}"),
        "unpacking" => "Decrypting and unpacking…".into(),
        _ => "Working…".into(),
    }
}

/// bkBar: the share done, or -1 when the phase can't say (it slides).
pub fn bar_frac(r: Option<&Value>) -> f32 {
    let Some(r) = r else { return 0.0 };
    let total = kit::n(r, "total");
    let ph = kit::s(r, "phase");
    if total > 0.0 && ph != "unpacking" && ph != "collecting" && ph != "tidying" { (kit::n(r, "done") / total).min(1.0) as f32 } else { -1.0 }
}

fn line(key: &str, bold: &str, text: &str, tone: &str) -> OpsBkLine {
    OpsBkLine { kind: "line".into(), key: key.into(), bold: bold.into(), text: text.into(), tone: tone.into() }
}
fn kind(k: &str, text: &str, tone: &str) -> OpsBkLine { OpsBkLine { kind: k.into(), text: text.into(), tone: tone.into(), ..Default::default() } }

/// The status box (bkStatusHtml), with `when` for the dates (bkWhen).
pub fn status_lines(d: &Value, now: f64, when: &dyn Fn(f64) -> String) -> Vec<OpsBkLine> {
    let mut out = Vec::new();
    let running = d.get("running").filter(|x| x.is_object());
    let waiting = d.get("waiting").filter(|x| x.is_object());
    if let Some(r) = running {
        out.push(line("", if kit::s(r, "reason") == "restore" { "Restoring" } else { "Backing up" }, &phase_text(r), ""));
        out.push(kind("bar", "", ""));
    } else if waiting.is_some() {
        out.push(line("", "A scheduled backup is due.", "It starts once you have left Crundi.", ""));
        out.push(kind("waiting", "", ""));
    }
    if d.get("staged").is_some_and(kit::truthy) {
        out.push(line("", "A restore is ready.", "It finishes when Crundi restarts.", ""));
        out.push(kind("staged", if d.get("restartMode").is_some_and(kit::truthy) { "restart" } else { "" }, ""));
    }
    if let Some(a) = d.get("restoreAttempt").filter(|x| x.is_object()) {
        if !ok(a) && running.is_none() { out.push(line("Restore", &kit::s(a, "error"), "", "bad")); }
    }
    let configured = d.get("configured").is_some_and(kit::truthy);
    if let Some(l) = d.get("last").filter(|x| x.is_object()) {
        let at = c::ms_of(l, "at").unwrap_or(0.0);
        if ok(l) {
            let how = if kit::s(l, "reason") == "scheduled" { "scheduled" } else { "made by hand" };
            out.push(line("Last backup", &when(at), &format!("{}, {} files, {how}", c::fmt_bytes(kit::n(l, "size")), kit::s(l, "files")), "good"));
        } else {
            out.push(line("Last backup", &format!("Failed {}", when(at)), &kit::s(l, "error"), "bad"));
        }
    } else if configured {
        out.push(line("Last backup", "", "None yet", ""));
    }
    match c::ms_of(d, "nextAt").filter(|x| *x > 0.0) {
        Some(n) => out.push(line("Next backup", &when(n), &format!("{} time", kit::s(d, "timezone")).trim().to_string(), "")),
        None => out.push(line("Next backup", "", if configured { "Scheduled backups are off" } else { "Set up the storage and a passphrase below" }, "")),
    }
    if let Some(r) = d.get("restored").filter(|x| x.is_object()) {
        let at = c::ms_of(r, "at").unwrap_or(0.0);
        if now - at < 7.0 * 86400000.0 {
            let made = c::ms_of(r, "backupCreatedAt").unwrap_or(0.0);
            let host = kit::s(r, "fromHost");
            let on = if host.is_empty() { String::new() } else { format!(" on {host}") };
            out.push(line("Restored", "", &format!("{} from the backup made {}{on}. What it replaced is in {}.", when(at), when(made), kit::s(r, "safetyCopy")), ""));
        }
    }
    if running.is_none() && waiting.is_none() { out.push(kind("run", "", if configured { "on" } else { "" })); }
    out
}

// ─── Settings → Backup & restore ───

pub fn render() {
    kit::call(|api| api.get("/api/backup"), |r| {
        match r {
            Ok(d) if ok(&d) => { bk(|b| b.st = Some(d)); draw_form(); }
            Ok(d) => { g(|g| { g.set_bk_state("error".into()); g.set_bk_load_error(c::err_of(&d, "Could not load the backup settings").into()); }); }
            Err(e) => { g(|g| { g.set_bk_state("error".into()); g.set_bk_load_error(e.into()); }); }
        }
        draw_notice();
    });
}

fn draw_form() {
    let Some(d) = bk(|b| b.st.clone()) else { return };
    let st = d.get("storage").cloned().unwrap_or_default();
    let sch = d.get("schedule").cloned().unwrap_or_default();
    g(|g| {
        g.set_bk_state("data".into());
        g.set_bk_endpoint(kit::s(&st, "endpoint").into());
        g.set_bk_bucket(kit::s(&st, "bucket").into());
        g.set_bk_region(kit::s(&st, "region").into());
        g.set_bk_prefix(kit::s(&st, "prefix").into());
        g.set_bk_style(if st.get("pathStyle") == Some(&Value::Bool(false)) { 1 } else { 0 });
        g.set_bk_keyid(kit::s(&st, "accessKeyId").into());
        g.set_bk_secret("".into());
        g.set_bk_secret_ph(if st.get("hasSecret").is_some_and(kit::truthy) { "Saved. Type to replace it".into() } else { "".into() });
        g.set_bk_test_out("".into());
        let has = d.get("hasPassphrase").is_some_and(kit::truthy);
        g.set_bk_pass_label(if has { "New passphrase".into() } else { "Passphrase".into() });
        g.set_bk_pass_ph(if has { "Set. Type to change it".into() } else { "At least 8 characters".into() });
        g.set_bk_pass("".into());
        g.set_bk_pass2("".into());
        g.set_bk_on(sch.get("enabled").is_some_and(kit::truthy));
        g.set_bk_freq(match kit::s(&sch, "frequency").as_str() { "weekly" => 1, "hours" => 2, _ => 0 });
        g.set_bk_weekday(kit::n(&sch, "weekday") as i32);
        g.set_bk_hours(kit::s(&sch, "everyHours").into());
        g.set_bk_time(kit::s(&sch, "time").into());
        g.set_bk_keep(kit::s(&d, "keep").into());
        g.set_bk_tz(kit::s(&d, "timezone").into());
        g.set_bk_save_out("".into());
    });
    draw_status();
    draw_items();
}

fn draw_status() {
    let Some(d) = bk(|b| b.st.clone()) else { return };
    LINES.with(|m| kit::sync_rows(m, status_lines(&d, c::now_ms(), &c::when)));
    g(|g| g.set_bk_frac(bar_frac(d.get("running").filter(|x| x.is_object()))));
}

fn draw_items() {
    let (items, ck) = bk(|b| (b.items.clone(), b.confirm_key.clone()));
    g(|g| {
        match &items {
            None => { g.set_bk_items_state("".into()); g.set_bk_list_btn("Show".into()); }
            Some(Err(e)) => { g.set_bk_items_state("error".into()); g.set_bk_items_error(e.clone().into()); g.set_bk_list_btn("Refresh".into()); }
            Some(Ok(v)) if v.is_empty() => { g.set_bk_items_state("empty".into()); g.set_bk_list_btn("Refresh".into()); }
            Some(Ok(_)) => { g.set_bk_items_state("data".into()); g.set_bk_list_btn("Refresh".into()); }
        }
        if !ck.is_empty() { g.set_bk_restore_pass("".into()); }
    });
    let rows = match items { Some(Ok(v)) => v.iter().map(|it| OpsBkItem {
        key: kit::s(it, "key").into(),
        when: c::ms_of(it, "at").map(c::medium_short).unwrap_or_default().into(),
        tag: if kit::s(it, "reason") == "scheduled" { "Scheduled".into() } else { "By hand".into() },
        size: c::fmt_bytes(kit::n(it, "size")).into(),
        host: kit::s(it, "host").into(),
        confirming: kit::s(it, "key") == ck,
    }).collect(), _ => vec![] };
    ITEMS.with(|m| kit::sync_rows(m, rows));
}

fn load_items() {
    g(|g| g.set_bk_list_btn("Loading…".into()));
    kit::call(|api| api.get("/api/backup/list"), |r| {
        let items = match r { Ok(d) if ok(&d) => Ok(d.get("items").and_then(|x| x.as_array()).cloned().unwrap_or_default()), Ok(d) => Err(c::err_of(&d, "Could not list the backups")), Err(e) => Err(e) };
        bk(|b| b.items = Some(items));
        draw_items();
    });
}

fn storage() -> Value {
    let region = get(|g| g.get_bk_region()).trim().to_string();
    json!({
        "endpoint": get(|g| g.get_bk_endpoint()).trim(), "bucket": get(|g| g.get_bk_bucket()).trim(),
        "region": if region.is_empty() { "auto".to_string() } else { region },
        "prefix": get(|g| g.get_bk_prefix()).trim(), "accessKeyId": get(|g| g.get_bk_keyid()).trim(),
        "secretAccessKey": get(|g| g.get_bk_secret()).trim(),
        "pathStyle": kit::ui().map(|w| w.global::<OpsSkills>().get_bk_style() != 1).unwrap_or(true),
    })
}

/// Number(x) as JSON: a number, 0 for blank, null for anything else (NaN).
pub fn number(s: &str) -> Value {
    let t = s.trim();
    if t.is_empty() { return json!(0); }
    t.parse::<f64>().ok().map(|n| if n.fract() == 0.0 { json!(n as i64) } else { json!(n) }).unwrap_or(Value::Null)
}

fn bk_busy(on: bool) { g(|g| g.set_bk_busy(on)); }

pub fn act(action: &str, arg: &str) {
    let arg = arg.to_string();
    match action {
        "bk-run" | "bkn-run" => {
            let notice = action == "bkn-run";
            if notice { g(|g| g.set_bkn_busy(true)); } else { bk_busy(true); }
            c::post("/api/backup/run", json!({}), move |d| {
                if notice { g(|g| g.set_bkn_busy(false)); }
                if !ok(&d) { kit::toast_error(&c::err_of(&d, "Could not start the backup")); bk_busy(false); }
                else if !notice { /* stays off until the status says it is running */ slint::Timer::single_shot(Duration::from_secs(2), || bk_busy(false)); }
            });
        }
        "bk-skip" | "bkn-skip" => {
            let notice = action == "bkn-skip";
            if notice { g(|g| g.set_bkn_busy(true)); }
            c::post("/api/backup/skip", json!({}), move |d| {
                if notice {
                    g(|g| g.set_bkn_busy(false));
                    if !ok(&d) { kit::toast_error(&c::err_of(&d, "Could not skip it")); } else { kit::toast("Skipped. The next backup runs on schedule."); }
                }
            });
        }
        "bkn-hide" => { bk(|b| b.notice_hidden = b.st.as_ref().and_then(|s| s.get("waiting")).map(|w| kit::n(w, "slot")).unwrap_or(0.0)); draw_notice(); }
        "bk-restart" => c::post("/api/backup/restart", json!({}), |d| if !ok(&d) { kit::toast_error(&c::err_of(&d, "Could not restart")); } else { kit::toast("Restarting to finish the restore"); }),
        "bk-cancel-restore" => c::post("/api/backup/cancel-restore", json!({}), |_| render()),
        "bk-test" => {
            g(|g| { g.set_bk_test_out("Checking…".into()); g.set_bk_test_tone("".into()); });
            c::post("/api/backup/test", json!({ "storage": storage() }), |d| g(|g| {
                if ok(&d) { g.set_bk_test_out("Connected. The bucket can be listed.".into()); g.set_bk_test_tone("good".into()); }
                else { g.set_bk_test_out(kit::s(&d, "error").into()); g.set_bk_test_tone("bad".into()); }
            }));
        }
        "bk-save" => save(),
        "bk-list" => load_items(),
        "bk-restore-ask" => { bk(|b| b.confirm_key = arg); draw_items(); }
        "bk-restore-cancel" => { bk(|b| b.confirm_key.clear()); draw_items(); }
        "bk-restore-go" => {
            bk_busy(true);
            let pass = get(|g| g.get_bk_restore_pass());
            c::post("/api/backup/restore", json!({ "key": arg, "passphrase": pass }), |d| {
                bk_busy(false);
                if !ok(&d) { kit::toast_error(&c::err_of(&d, "Could not start the restore")); return; }
                bk(|b| b.confirm_key.clear());
                draw_items();
                kit::toast("Restoring. Crundi restarts when the backup is unpacked.");
            });
        }
        "bk-delete" => kit::confirm(kit::Confirm { title: "Delete backup", message: "Delete this backup from storage? This cannot be undone.", ok: "Delete", danger: true }, move |yes| {
            if !yes { return; }
            c::post("/api/backup/delete", json!({ "key": arg }), |d| {
                if !ok(&d) { kit::toast_error(&c::err_of(&d, "Could not delete it")); return; }
                kit::toast("Backup deleted");
                load_items();
            });
        }),
        _ => {}
    }
}

fn save() {
    let (p1, p2) = (get(|g| g.get_bk_pass()), get(|g| g.get_bk_pass2()));
    if p1 != p2 { g(|g| { g.set_bk_save_out("The two passphrases differ".into()); g.set_bk_save_tone("bad".into()); }); return; }
    let Some(w) = kit::ui() else { return };
    let gl = w.global::<OpsSkills>();
    let time = gl.get_bk_time().to_string();
    let freq = ["daily", "weekly", "hours"][gl.get_bk_freq().clamp(0, 2) as usize];
    let body = json!({
        "storage": storage(), "passphrase": p1,
        "schedule": {
            "enabled": gl.get_bk_on(),
            "frequency": freq,
            "time": if time.trim().is_empty() { "03:00".to_string() } else { time.trim().to_string() },
            "weekday": gl.get_bk_weekday(),
            "everyHours": number(gl.get_bk_hours().as_str()),
        },
        "keep": number(gl.get_bk_keep().as_str()),
    });
    c::post("/api/backup/settings", body, |d| {
        if !ok(&d) { g(|g| { g.set_bk_save_out(kit::s(&d, "error").into()); g.set_bk_save_tone("bad".into()); }); return; }
        kit::toast_ok("Backup settings saved");
        if let Some(cfg) = d.get("config").and_then(|x| x.as_object()).cloned() { merge(cfg); }
        render();
    });
}

fn merge(st: Map<String, Value>) {
    bk(|b| {
        let mut o = b.st.take().and_then(|v| v.as_object().cloned()).unwrap_or_default();
        for (k, v) in st { o.insert(k, v); }
        b.st = Some(Value::Object(o));
    });
}

/// A status push from the server ("backup"): Settings, and the notice.
pub fn on_status(st: &Value) {
    let Some(obj) = st.as_object().cloned() else { return };
    let prev = bk(|b| b.st.as_ref().and_then(|s| s.get("running")).filter(|x| x.is_object()).cloned());
    merge(obj);
    draw_status();
    let running = st.get("running").filter(|x| x.is_object()).is_some();
    let last = st.get("last").filter(|x| x.is_object());
    if prev.is_some() && !running {
        g(|g| g.set_bk_busy(false));
        if last.is_some() && bk(|b| b.items.is_some()) { load_items(); }
    }
    if let (Some(p), false, Some(l)) = (&prev, running, last) {
        if kit::s(p, "reason") != "restore" {
            if ok(l) { kit::toast_ok("Backup uploaded"); } else { kit::toast_error(&format!("Backup failed: {}", kit::s(l, "error"))); }
        }
    }
    if st.get("restarting").is_some_and(kit::truthy) { kit::toast("Restore ready. Crundi is restarting…"); }
    if let (Some(a), Some(p)) = (st.get("restoreAttempt").filter(|x| x.is_object()), &prev) {
        if !ok(a) && kit::s(p, "reason") == "restore" { kit::toast_error(&format!("Restore failed: {}", kit::s(a, "error"))); }
    }
    draw_notice();
}

fn draw_notice() {
    let vis = bk(|b| {
        let w = b.st.as_ref().and_then(|s| s.get("waiting")).filter(|x| x.is_object());
        w.is_some_and(|w| b.notice_hidden != kit::n(w, "slot"))
    });
    g(|g| g.set_bkn_visible(vis));
}

/// refreshBackupNotice: on sign-in and on every reconnect (the owner's).
pub fn refresh_notice() {
    kit::call(|api| api.get("/api/backup"), |r| {
        if let Ok(d) = r { if ok(&d) { if let Some(o) = d.as_object().cloned() { merge(o); draw_notice(); } } }
    });
}

// ─── Settings → Usage ───

pub fn usage_load() {
    kit::call(|api| api.get("/api/settings"), |r| match r {
        Ok(d) if ok(&d) => { bk(|b| b.us = Some(d.get("limitWarmup").cloned().unwrap_or(json!({ "enabled": false })))); usage_draw(); }
        Ok(d) => g(|g| { g.set_us_state("error".into()); g.set_us_error(c::err_of(&d, "Could not load the settings").into()); }),
        Err(e) => g(|g| { g.set_us_state("error".into()); g.set_us_error(e.into()); }),
    });
}

/// The warm-up's two lines: the last warm-up, and the window.
pub fn usage_lines(w: &Value, now: f64, fmt: &dyn Fn(f64) -> String) -> (String, String) {
    let last = c::ms_of(w, "lastWarmAt").filter(|x| *x > 0.0).map(|at| {
        let r = kit::s(w, "lastWarmResult");
        format!("Last: {} · {}", if r.is_empty() { "—".into() } else { r }, fmt(at))
    }).unwrap_or_default();
    let reset = c::ms_of(w, "lastKnownReset").map(|at| if at > now { format!("Current window resets {}", fmt(at)) } else { format!("No window open · last one ended {}", fmt(at)) }).unwrap_or_default();
    (last, reset)
}

fn usage_draw() {
    let Some(w) = bk(|b| b.us.clone()) else { return };
    let (last, reset) = usage_lines(&w, c::now_ms(), &c::locale);
    g(|g| {
        g.set_us_state("data".into());
        g.set_us_on(w.get("enabled").is_some_and(kit::truthy));
        g.set_us_last(last.into());
        g.set_us_reset(reset.into());
    });
}

fn usage_set(on: bool) {
    g(|g| g.set_us_on(on));
    c::post("/api/settings", json!({ "limitWarmup": on }), move |d| {
        if !ok(&d) {
            g(|g| g.set_us_on(!on));
            kit::toast_error(&format!("Could not change that: {}", c::err_of(&d, "Failed")));
            return;
        }
        bk(|b| if let Some(u) = &mut b.us { u["enabled"] = json!(on); });
        let gone = kit::n(&d, "clearedLimitReset") as i64;
        if on { kit::toast_ok("Crundi will keep the 5-hour window warm"); }
        else if gone > 0 { kit::toast_ok(&format!("Window warm-up off - cleared {gone}{}", if gone == 1 { " queued message" } else { " queued messages" })); }
        else { kit::toast_ok("Window warm-up off"); }
    });
}

/// The switch: on at once; off asks first when messages wait on the reset.
pub fn usage_toggle(on: bool) {
    if on { usage_set(true); return; }
    kit::call(|api| api.get("/api/settings"), |r| {
        let queued = r.ok().map(|d| kit::n(&d, "queuedLimitReset") as i64).unwrap_or(0);
        if queued <= 0 { usage_set(false); return; }
        let subject = if queued == 1 { "1 scheduled message is".to_string() } else { format!("{queued} scheduled messages are") };
        let msg = format!("{subject} waiting to send when the limit resets. Warm-up is what notices the reset, so turning it off clears {}.", if queued == 1 { "it" } else { "them" });
        // Until the answer the switch shows what it is.
        g(|g| g.set_us_on(true));
        kit::confirm(kit::Confirm { title: "Turn off window warm-up?", message: &msg, ok: "Turn off", danger: true }, |yes| if yes { usage_set(false); });
    });
}

// ─── Restore at sign-in ───

/// Open "Restore from a backup" over the sign-in page, for `server` (its base URL).
pub fn open_login_restore(server: &str) {
    bk(|b| { b.lr_base = server.trim_end_matches('/').to_string(); b.lr_items.clear(); });
    LR.with(|m| kit::sync_rows(m, vec![]));
    g(|g| {
        g.set_lr_list("".into()); g.set_lr_error("".into()); g.set_lr_progress(false); g.set_lr_going(false);
        g.set_lr_finding(false); g.set_lr_pass("".into()); g.set_lr_picked(0); g.set_lr_open(true);
    });
}

fn lr_storage() -> Value {
    let Some(w) = kit::ui() else { return json!({}) };
    let g = w.global::<OpsSkills>();
    let region = g.get_lr_region().trim().to_string();
    json!({
        "endpoint": g.get_lr_endpoint().trim(), "bucket": g.get_lr_bucket().trim(),
        "region": if region.is_empty() { "auto".to_string() } else { region },
        "prefix": g.get_lr_prefix().trim(), "accessKeyId": g.get_lr_key().trim(), "secretAccessKey": g.get_lr_secret().trim(),
        "pathStyle": !g.get_lr_vhost(),
    })
}

/// An unauthenticated call to the server being restored, off the UI thread.
fn open(method: &'static str, path: &'static str, body: Option<Value>, done: impl FnOnce(Result<Value, String>) + Send + 'static) {
    let base = bk(|b| b.lr_base.clone());
    kit::bg(move || crate::api::open_call(&base, method, path, body.as_ref()).map_err(|e| match e {
        kit::ApiError::Net(m) => format!("Could not reach the server: {m}"),
        e => e.to_string(),
    }), done);
}

pub fn lr_act(action: &str) {
    match action {
        "lr-back" => g(|g| g.set_lr_open(false)),
        "lr-find" => {
            g(|g| { g.set_lr_error("".into()); g.set_lr_finding(true); });
            open("POST", "/api/backup/setup/list", Some(json!({ "storage": lr_storage() })), |r| {
                g(|g| g.set_lr_finding(false));
                let d = match r { Ok(d) => d, Err(e) => { g(|g| g.set_lr_error(e.into())); return; } };
                if !ok(&d) { g(|g| g.set_lr_error(c::err_of(&d, "Could not list the bucket").into())); return; }
                let items = d.get("items").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                let rows = items.iter().map(|it| OpsLrItem {
                    key: kit::s(it, "key").into(), when: c::ms_of(it, "at").map(c::medium_short).unwrap_or_default().into(),
                    host: kit::s(it, "host").into(), size: c::fmt_bytes(kit::n(it, "size")).into(),
                }).collect();
                LR.with(|m| kit::sync_rows(m, rows));
                let empty = items.is_empty();
                bk(|b| b.lr_items = items);
                g(|g| { g.set_lr_picked(0); g.set_lr_list(if empty { "empty".into() } else { "data".into() }); });
            });
        }
        "lr-go" => {
            let Some(w) = kit::ui() else { return };
            let gl = w.global::<OpsSkills>();
            let key = bk(|b| b.lr_items.get(gl.get_lr_picked().max(0) as usize).map(|x| kit::s(x, "key")));
            let Some(key) = key else { gl.set_lr_error("Choose a backup".into()); return };
            let pass = gl.get_lr_pass().to_string();
            if pass.is_empty() { gl.set_lr_error("Enter the passphrase the backup was made with".into()); return; }
            gl.set_lr_error("".into());
            gl.set_lr_going(true);
            open("POST", "/api/backup/setup/restore", Some(json!({ "storage": lr_storage(), "key": key, "passphrase": pass })), |r| {
                let d = match r { Ok(d) => d, Err(e) => { g(|g| { g.set_lr_error(e.into()); g.set_lr_going(false); }); return; } };
                if !ok(&d) { g(|g| { g.set_lr_error(c::err_of(&d, "Could not start the restore").into()); g.set_lr_going(false); }); return; }
                g(|g| { g.set_lr_progress(true); g.set_lr_phase("".into()); g.set_lr_frac(-1.0); });
                lr_poll(d.get("restartMode").is_some_and(kit::truthy));
            });
        }
        _ => {}
    }
}

fn lr_poll(mode: bool) {
    open("GET", "/api/backup/setup/status", None, move |r| {
        if !kit::ui().is_some_and(|w| w.global::<OpsSkills>().get_lr_open()) { return; }
        let d = r.ok();
        if let Some(run) = d.as_ref().and_then(|d| d.get("running")).filter(|x| x.is_object()) {
            g(|g| { g.set_lr_phase(phase_text(run).into()); g.set_lr_frac(bar_frac(Some(run))); });
            slint::Timer::single_shot(Duration::from_millis(800), move || lr_poll(mode));
            return;
        }
        if let Some(a) = d.as_ref().and_then(|d| d.get("restoreAttempt")).filter(|x| x.is_object()) {
            if !ok(a) { g(|g| { g.set_lr_progress(false); g.set_lr_error(kit::s(a, "error").into()); g.set_lr_going(false); }); return; }
            if !mode { g(|g| { g.set_lr_phase("The backup is ready. Restart Crundi to finish restoring it.".into()); g.set_lr_frac(0.0); }); return; }
            g(|g| { g.set_lr_phase("Restored. Crundi is restarting; sign in with the backup’s password when it is back.".into()); g.set_lr_frac(0.0); });
            bk(|b| b.lr_since = c::now_ms());
            lr_wait_back();
            return;
        }
        // Not started yet, or the server is already restarting.
        slint::Timer::single_shot(Duration::from_millis(800), move || lr_poll(mode));
    });
}

fn lr_wait_back() {
    slint::Timer::single_shot(Duration::from_millis(2500), || {
        open("GET", "/api/auth/methods", None, |r| {
            if let Ok(m) = r { if ok(&m) && !m.get("setupRequired").is_some_and(kit::truthy) {
                // Back: the sign-in page asks the server again (the web reloads the page).
                g(|g| g.set_lr_open(false));
                let base = bk(|b| b.lr_base.clone());
                if let Some(w) = kit::ui() { w.invoke_check_server(base.into()); }
                return;
            } }
            if c::now_ms() - bk(|b| b.lr_since) < 180000.0 { lr_wait_back(); }
            else { g(|g| g.set_lr_phase("Crundi has not come back yet. Reload this page in a moment.".into())); }
        });
    });
}

pub fn props() -> Value {
    let Some(w) = kit::ui() else { return Value::Null };
    let g = w.global::<OpsSkills>();
    let lines: Vec<Value> = LINES.with(|m| (0..slint::Model::row_count(&**m)).filter_map(|i| slint::Model::row_data(&**m, i)).map(|l| json!({ "kind": l.kind.as_str(), "key": l.key.as_str(), "bold": l.bold.as_str(), "text": l.text.as_str() })).collect());
    json!({
        "backup": {
            "state": g.get_bk_state().as_str(), "error": g.get_bk_load_error().as_str(), "lines": lines,
            "items": g.get_bk_items_state().as_str(), "itemsError": g.get_bk_items_error().as_str(), "listBtn": g.get_bk_list_btn().as_str(),
            "testOut": g.get_bk_test_out().as_str(), "testTone": g.get_bk_test_tone().as_str(),
            "saveOut": g.get_bk_save_out().as_str(), "notice": g.get_bkn_visible(),
            "endpoint": g.get_bk_endpoint().as_str(), "freq": g.get_bk_freq(), "keep": g.get_bk_keep().as_str(), "passLabel": g.get_bk_pass_label().as_str(),
        },
        "usage": { "state": g.get_us_state().as_str(), "on": g.get_us_on(), "last": g.get_us_last().as_str(), "reset": g.get_us_reset().as_str() },
        "restore": { "open": g.get_lr_open(), "list": g.get_lr_list().as_str(), "error": g.get_lr_error().as_str(), "finding": g.get_lr_finding(), "progress": g.get_lr_progress(), "phase": g.get_lr_phase().as_str() },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(ms: f64) -> String { format!("<{}>", ms as i64) }

    #[test]
    fn phases() {
        assert_eq!(phase_text(&json!({ "phase": "uploading", "done": 1, "total": 4 })), "Uploading… 25%");
        assert_eq!(phase_text(&json!({ "phase": "collecting" })), "Gathering files…");
        assert_eq!(phase_text(&json!({})), "Working…");
        assert_eq!(bar_frac(Some(&json!({ "phase": "uploading", "done": 1, "total": 4 }))), 0.25);
        assert_eq!(bar_frac(Some(&json!({ "phase": "unpacking", "done": 1, "total": 4 }))), -1.0);
    }

    #[test]
    fn status_unconfigured() {
        let l = status_lines(&json!({ "configured": false }), 0.0, &w);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].text.as_str(), "Set up the storage and a passphrase below");
        assert_eq!(l[1].kind.as_str(), "run");
        assert_eq!(l[1].tone.as_str(), "");
    }

    #[test]
    fn status_configured_running_and_staged() {
        let d = json!({ "configured": true, "running": { "reason": "manual", "phase": "packing", "done": 1, "total": 2 },
            "staged": true, "restartMode": "systemd", "nextAt": 5000, "timezone": "UTC",
            "last": { "ok": true, "at": 1000, "size": 2048, "files": 12, "reason": "scheduled" } });
        let l = status_lines(&d, 0.0, &w);
        let kinds: Vec<&str> = l.iter().map(|x| x.kind.as_str()).collect();
        assert_eq!(kinds, ["line", "bar", "line", "staged", "line", "line"]);
        assert_eq!(l[0].bold.as_str(), "Backing up");
        assert_eq!(l[0].text.as_str(), "Packing and encrypting… 50%");
        assert_eq!(l[3].text.as_str(), "restart");
        assert_eq!(l[4].text.as_str(), "2.0 KB, 12 files, scheduled");
        assert_eq!(l[5].bold.as_str(), "<5000>");
        assert_eq!(l[5].text.as_str(), "UTC time");
    }

    #[test]
    fn status_waiting_and_failed() {
        let d = json!({ "configured": true, "waiting": { "slot": 3 }, "last": { "ok": false, "at": 1000, "error": "boom" }, "restoreAttempt": { "ok": false, "error": "bad pass" } });
        let l = status_lines(&d, 0.0, &w);
        assert_eq!(l[1].kind.as_str(), "waiting");
        assert_eq!(l[2].key.as_str(), "Restore");
        assert_eq!(l[2].bold.as_str(), "bad pass");
        assert_eq!(l[3].bold.as_str(), "Failed <1000>");
        assert!(l.iter().all(|x| x.kind.as_str() != "run"));
    }

    #[test]
    fn numbers_as_javascript() {
        assert_eq!(number("12"), json!(12));
        assert_eq!(number(""), json!(0));
        assert_eq!(number("x"), Value::Null);
    }

    #[test]
    fn warmup_lines() {
        let (a, b) = usage_lines(&json!({ "lastWarmAt": 1000, "lastWarmResult": "ok", "lastKnownReset": 9000 }), 5000.0, &w);
        assert_eq!(a, "Last: ok · <1000>");
        assert_eq!(b, "Current window resets <9000>");
        let (_, b) = usage_lines(&json!({ "lastKnownReset": 1000 }), 5000.0, &w);
        assert_eq!(b, "No window open · last one ended <1000>");
    }
}
