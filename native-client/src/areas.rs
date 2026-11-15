//! The area registry (AREAS.md): app.rs tells this file what happens (the
//! window starts, sign-in, a project, a tab, the live link) and it tells
//! each area, in its own module (src/workspace, src/planning, src/ops,
//! src/desktop). The test driver reaches an area's actions and props
//! through here too. Only the Lead changes this file; an area changes only
//! its own module.
//!
//! Every area module has the same functions:
//!
//!   init(&MainWindow)          once, at start: set models, wire callbacks
//!   signed_in() / signed_out() a session starts or ends
//!   project(alias)             the project changed ("" none)
//!   tab_shown(tab)             a tab came on show (any tab: check yours)
//!   event(&AreaEvent)          the live link (see below)
//!   test_ui(name, args)        the driver's "<area>:<name>" (None: unknown)
//!   props()                    the driver's view of the area (JSON)

#![allow(dead_code)]

use crate::{live::Event, MainWindow};
use serde_json::{json, Value};
use slint::ComponentHandle;

/// What the live link tells the areas.
pub enum AreaEvent<'a> {
    /// /ws and /api/events are up again (true) or lost: resend subscriptions.
    Connected(bool),
    /// The event stream's "state" (projects, terminals, services…).
    State(&'a Value),
    /// Another /api/events event by name ("usage", "inbox", "kanban", …).
    Server(&'a str, &'a Value),
    /// Another /ws message by type (the whole message).
    Ws(&'a str, &'a Value),
}

pub const AREAS: [&str; 4] = ["workspace", "planning", "ops", "desktop"];

pub fn init(w: &MainWindow) {
    crate::kit::init(w);
    w.on_tab_shown(|t| tab_shown(t.as_str()));
    let sh = w.global::<crate::Shell>();
    sh.on_sign_out(|| crate::app::sign_out(""));
    crate::workspace::init(w);
    crate::planning::init(w);
    crate::ops::init(w);
    crate::desktop::init(w);
}

pub fn signed_in(server: &str) {
    if let Some(w) = crate::kit::ui() { w.global::<crate::Shell>().set_server(server.into()); }
    crate::workspace::signed_in();
    crate::planning::signed_in();
    crate::ops::signed_in();
    crate::desktop::signed_in();
}

pub fn signed_out() {
    if let Some(w) = crate::kit::ui() {
        let sh = w.global::<crate::Shell>();
        sh.set_modal_workspace(false); sh.set_modal_planning(false); sh.set_modal_ops(false); sh.set_modal_desktop(false);
        let d = w.global::<crate::DialogUi>();
        if d.get_open() { crate::kit::dialog_answer(false, String::new()); }
        w.global::<crate::MenuUi>().set_open(false);
    }
    crate::workspace::signed_out();
    crate::planning::signed_out();
    crate::ops::signed_out();
    crate::desktop::signed_out();
}

pub fn project(alias: &str) {
    crate::workspace::project(alias);
    crate::planning::project(alias);
    crate::ops::project(alias);
    crate::desktop::project(alias);
}

pub fn tab_shown(tab: &str) {
    crate::workspace::tab_shown(tab);
    crate::planning::tab_shown(tab);
    crate::ops::tab_shown(tab);
    crate::desktop::tab_shown(tab);
}

/// From app.rs's on_live, before it handles the event itself.
pub fn live(e: &Event) {
    let ev = match e {
        Event::Connected(on) => AreaEvent::Connected(*on),
        Event::State(v) => AreaEvent::State(v),
        Event::Usage(v) => AreaEvent::Server("usage", v),
        Event::Inbox(v) => AreaEvent::Server("inbox", v),
        Event::ChatSecrets(v) => AreaEvent::Server("chat-secrets", v),
        Event::SecretRequests(v) => AreaEvent::Server("secret-requests", v),
        Event::Server(n, v) => AreaEvent::Server(n, v),
        Event::Ws(t, v) => AreaEvent::Ws(t, v),
        Event::Chat(_) | Event::SignedOut => return,
    };
    crate::workspace::event(&ev);
    crate::planning::event(&ev);
    crate::ops::event(&ev);
    crate::desktop::event(&ev);
}

/// The driver's "<area>:<name>" action.
pub fn test_ui(area: &str, name: &str, args: &[String]) -> Value {
    let r = match area {
        "workspace" => crate::workspace::test_ui(name, args),
        "planning" => crate::planning::test_ui(name, args),
        "ops" => crate::ops::test_ui(name, args),
        "desktop" => crate::desktop::test_ui(name, args),
        _ => return json!({ "ok": false, "error": format!("unknown area {area}") }),
    };
    r.unwrap_or_else(|| json!({ "ok": false, "error": format!("unknown ui action {area}:{name}") }))
}

pub fn props_of(area: &str) -> Value {
    match area {
        "workspace" => crate::workspace::props(),
        "planning" => crate::planning::props(),
        "ops" => crate::ops::props(),
        "desktop" => crate::desktop::props(),
        _ => Value::Null,
    }
}

pub fn props() -> Value {
    Value::Object(AREAS.iter().map(|a| (a.to_string(), props_of(a))).collect())
}
