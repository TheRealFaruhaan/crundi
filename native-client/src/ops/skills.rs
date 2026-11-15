//! Ops area: Settings' Skills, Backup / sign-in restore and Usage sections (drawn inside Settings).
//! UI: ui/ops/skills.slint (global OpsSkills).
//!
//! skills/list.rs: Skills; skills/backup.rs: Backup & restore, the backup
//! notice, Usage (window warm-up) and the sign-in restore;
//! skills/common.rs: what these and Info (src/ops/info.rs) share.

use crate::{areas::AreaEvent, kit, MainWindow};
use serde_json::{json, Value};
use slint::ComponentHandle;

pub mod common;
mod list;
mod backup;

pub use backup::open_login_restore;

pub fn init(w: &MainWindow) {
    list::init(w);
    backup::init(w);
    w.global::<crate::OpsSkills>().on_act(|a, b| act(a.as_str(), b.as_str()));
}

fn act(action: &str, arg: &str) {
    match action {
        "us-toggle" => backup::usage_toggle(arg == "on"),
        a if a.starts_with("lr-") => backup::lr_act(a),
        a if a.starts_with("bk") => backup::act(a, arg),
        a => list::act(a, arg),
    }
}

pub fn signed_in() {
    backup::refresh_notice();
}

pub fn signed_out() {
    list::signed_out();
    backup::signed_out();
}

pub fn project(_alias: &str) {}

/// Settings came on show: its three sections load (renderSettings renders
/// Skills, Backup and Usage every time it opens).
pub fn tab_shown(tab: &str) {
    if tab != "settings" { return; }
    list::load(None);
    backup::render();
    backup::usage_load();
}

pub fn event(e: &AreaEvent) {
    match e {
        AreaEvent::Server("backup", v) => backup::on_status(v),
        AreaEvent::Connected(true) => backup::refresh_notice(),
        _ => {}
    }
}

/// The driver's "ops:<name>" actions this screen knows (None: not mine).
///   skills-act <action> <arg>     a button (OpsSkills.act), as a click
///   skills-field <name> <text>    type into a field (the ones below)
///   skills-load                   load the three sections (as opening Settings)
///   skills-lr-open <server>       open the sign-in restore
///   skills-url                    the last link opened
pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    match name {
        "skills-act" => { act(&a(0), &a(1)); Some(json!({ "ok": true })) }
        "skills-load" => { tab_shown("settings"); Some(json!({ "ok": true })) }
        "skills-lr-open" => { open_login_restore(&a(0)); Some(json!({ "ok": true })) }
        "skills-url" => Some(json!({ "ok": true, "url": common::last_opened() })),
        "skills-field" => {
            let w = kit::ui()?;
            let g = w.global::<crate::OpsSkills>();
            let v: slint::SharedString = a(1).into();
            match a(0).as_str() {
                "new-name" => g.set_sk_new_name(v), "new-desc" => g.set_sk_new_desc(v),
                "link" => g.set_sk_link_text(v), "pending-name" => g.set_sk_pending_name(v),
                "rename" => g.set_sd_rename(v), "as" => g.set_sd_as(v), "newfile" => g.set_sd_newfile(v),
                "to" => g.set_sd_to(a(1).parse().unwrap_or(0)),
                "editor" => g.set_ed_text(v),
                "bk-endpoint" => g.set_bk_endpoint(v), "bk-bucket" => g.set_bk_bucket(v), "bk-keyid" => g.set_bk_keyid(v),
                "bk-secret" => g.set_bk_secret(v), "bk-pass" => g.set_bk_pass(v), "bk-pass2" => g.set_bk_pass2(v),
                "bk-keep" => g.set_bk_keep(v), "bk-hours" => g.set_bk_hours(v), "bk-time" => g.set_bk_time(v),
                "bk-freq" => g.set_bk_freq(a(1).parse().unwrap_or(0)), "bk-on" => g.set_bk_on(a(1) == "true"),
                "lr-endpoint" => g.set_lr_endpoint(v), "lr-bucket" => g.set_lr_bucket(v), "lr-key" => g.set_lr_key(v),
                "lr-secret" => g.set_lr_secret(v), "lr-pass" => g.set_lr_pass(v),
                _ => return Some(json!({ "ok": false, "error": "unknown field" })),
            }
            Some(json!({ "ok": true }))
        }
        _ => None,
    }
}

/// This screen's part of the area's props (under "skills").
pub fn props() -> Value {
    let mut p = list::props();
    if let (Some(o), Value::Object(b)) = (p.as_object_mut(), backup::props()) { o.extend(b); }
    p
}
