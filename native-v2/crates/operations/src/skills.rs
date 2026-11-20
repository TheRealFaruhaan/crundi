//! Skills, Usage, Backup & restore (sections of Settings) and the sign-in
//! restore (Slint: src/ops/skills.rs + skills/, ui/ops/skills*.slint,
//! specs/ops/skills.md). settings.rs places the three sections in its
//! groups; `login_restore` is the sign-in page's "Restore from a backup
//! instead", a part the window places over the sign-in page.
//!
//! skills/list.rs: Skills; skills/backup.rs: Backup & restore, the backup
//! notice, Usage (window warm-up) and the sign-in restore; skills/common.rs:
//! what these, Settings and Info share. The state is one Global (`Sk`).

pub mod common;
mod list;
mod backup;

use common::{Fields, HasFields};
use crate::compat::AreaEvent;
use gpui::*;
use serde_json::{json, Value};

pub use backup::open_login_restore;

/// Every text box the driver can type into (its names).
const FIELDS: &[&str] = &["new-name", "new-desc", "link", "pending-name", "rename", "as", "newfile", "editor",
    "bk-endpoint", "bk-bucket", "bk-region", "bk-prefix", "bk-keyid", "bk-secret", "bk-pass", "bk-pass2", "bk-keep", "bk-hours", "bk-time", "bk-restore-pass",
    "lr-endpoint", "lr-bucket", "lr-region", "lr-prefix", "lr-key", "lr-secret", "lr-pass"];

#[derive(Default)]
pub struct Sk {
    pub(crate) l: list::List,
    pub(crate) b: backup::Bk,
    pub(crate) f: Fields,
}

/// The handle to the state (a Global holds only it: ARCHITECTURE.md rule 1).
struct SkH(Entity<Sk>);
impl Global for SkH {}
impl HasFields for Sk {
    fn handle(cx: &App) -> Option<Entity<Sk>> { cx.try_global::<SkH>().map(|h| h.0.clone()) }
    fn fields(&mut self) -> &mut Fields { &mut self.f }
}

/// The state's entity (made on first use); Settings' page observes it.
pub(crate) fn ent(cx: &mut App) -> Entity<Sk> {
    if let Some(h) = cx.try_global::<SkH>() { return h.0.clone(); }
    let mut s = Sk::default();
    s.l.target = "global".into();
    s.b.reset();
    let e = cx.new(|_| s);
    cx.set_global(SkH(e.clone()));
    e
}

fn ensure(cx: &mut App) { ent(cx); }

/// Change the state; the page draws again.
pub(crate) fn upd<R>(cx: &mut App, f: impl FnOnce(&mut Sk) -> R) -> R {
    let e = ent(cx);
    e.update(cx, |s, cx| { let r = f(s); cx.notify(); r })
}

/// Change what no view shows (no redraw).
#[allow(dead_code)]
pub(crate) fn quiet<R>(cx: &mut App, f: impl FnOnce(&mut Sk) -> R) -> R { let e = ent(cx); e.update(cx, |s, _| f(s)) }

pub(crate) fn get<R>(cx: &mut App, f: impl FnOnce(&Sk) -> R) -> R { let e = ent(cx); f(e.read(cx)) }

/// The state, while the page draws (after `ensure`).
pub(crate) fn st(cx: &App) -> &Sk { cx.global::<SkH>().0.read(cx) }

/// The scheduled-backup notice's layer, bottom left over the whole window
/// (the web's drawBackupNotice): a cached view, redrawn when the state
/// changes; empty (and taking no pointer) while the notice is hidden.
pub struct NoticeLayer { _sub: Subscription }
impl Render for NoticeLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("backup-notice");
        div().size_full().relative().children(backup_notice(window, cx))
    }
}

pub fn init(cx: &mut App) {
    let e = ent(cx);
    let layer = cx.new(|cx| NoticeLayer { _sub: cx.observe(&e, |_, _, cx| cx.notify()) });
    crundi_shell::register_overlay(cx, layer.into());
    crundi_shell::register_login_restore(cx, &e, open_login_restore, login_restore);
}

/// Settings → Skills (its fold's body).
pub fn skills_section(window: &mut Window, cx: &mut App) -> AnyElement { ensure(cx); list::render(window, cx) }
/// Settings → Usage.
pub fn usage_section(window: &mut Window, cx: &mut App) -> AnyElement { ensure(cx); backup::usage_render(window, cx) }
/// Settings → Backup & restore.
pub fn backup_section(window: &mut Window, cx: &mut App) -> AnyElement { ensure(cx); backup::render(window, cx) }
/// The sign-in page's restore, over the whole window while open (None when closed).
pub fn login_restore(window: &mut Window, cx: &mut App) -> Option<AnyElement> { ensure(cx); backup::lr_render(window, cx) }
/// The scheduled-backup notice, bottom left of the window (None when hidden).
pub fn backup_notice(window: &mut Window, cx: &mut App) -> Option<AnyElement> { ensure(cx); backup::notice_render(window, cx) }

/// No tab of its own: its sections are drawn by Settings.
pub fn view(_tab: &str, _window: &mut Window, _cx: &mut App) -> Option<AnyView> { None }

fn act(cx: &mut App, action: &str, arg: &str) {
    match action {
        "us-toggle" => backup::usage_toggle(cx, arg == "on"),
        a if a.starts_with("lr-") => backup::lr_act(cx, a),
        a if a.starts_with("bk") => backup::act(cx, a, arg),
        a => list::act(cx, a, arg),
    }
}

pub fn on_event(e: &AreaEvent, cx: &mut App) {
    match e {
        AreaEvent::Server("backup", v) => backup::on_status(cx, v),
        AreaEvent::Connected(true) => backup::refresh_notice(cx),
        _ => {}
    }
}

pub fn on_project(_alias: &str, _cx: &mut App) {}

pub fn on_signed_in(cx: &mut App) { backup::refresh_notice(cx); }

pub fn on_signed_out(cx: &mut App) {
    upd(cx, |s| {
        s.l = list::List::default();
        s.l.target = "global".into();
        s.f.set("editor", "");
        s.b.signed_out();
    });
}

/// Settings came on show: its three sections load (renderSettings renders
/// Skills, Backup and Usage every time it opens).
pub fn on_tab(tab: &str, cx: &mut App) {
    if tab != "settings" { return; }
    list::load(cx, None);
    backup::load(cx);
    backup::usage_load(cx);
}

/// The driver's "ops:<name>" actions this screen knows (None: not mine).
///   skills-act <action> <arg>     a button, as a click
///   skills-field <name> <text>    type into a field
///   skills-load                   load the three sections (as opening Settings)
///   skills-lr-open <server>       open the sign-in restore
///   skills-url                    the last link opened
pub fn test_ui(name: &str, args: &[String], _window: &mut Window, cx: &mut App) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    if !name.starts_with("skills-") { return None; }
    ensure(cx);
    match name {
        "skills-act" => act(cx, &a(0), &a(1)),
        "skills-load" => on_tab("settings", cx),
        "skills-lr-open" => open_login_restore(cx, &a(0)),
        "skills-url" => return Some(json!({ "ok": true, "url": common::last_opened() })),
        "skills-field" => {
            let v = a(1);
            let n = v.parse::<i32>().unwrap_or(0);
            match a(0).as_str() {
                "to" => upd(cx, |s| s.l.to = n.max(0) as usize),
                "bk-freq" => upd(cx, |s| s.b.freq = n.clamp(0, 2)),
                "bk-on" => upd(cx, |s| s.b.on = v == "true"),
                "bk-style" => upd(cx, |s| s.b.style = n.clamp(0, 1)),
                "bk-weekday" => upd(cx, |s| s.b.weekday = n.clamp(0, 6)),
                "lr-vhost" => upd(cx, |s| s.b.lr_vhost = v == "true"),
                "lr-picked" => upd(cx, |s| s.b.lr_picked = n.max(0) as usize),
                k => match FIELDS.iter().find(|x| **x == k) {
                    Some(k) => upd(cx, |s| s.f.set(k, v)),
                    None => return Some(json!({ "ok": false, "error": "unknown field" })),
                },
            }
        }
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// This screen's part of the area's props (under "skills").
pub fn props(_window: &mut Window, cx: &mut App) -> Value {
    ensure(cx);
    let mut p = list::props(cx);
    if let (Some(o), Value::Object(b)) = (p.as_object_mut(), backup::props(cx)) { o.extend(b); }
    p
}
