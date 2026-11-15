//! The desktop area (AREAS.md): the browser drawer and the Browsers tab, the native host's full browser protocol, and phase 6: notifications and the tray, the Inbox, the chat bubble, saved passwords, the homepool node, Cru.
//!
//! The registry (src/areas.rs) calls the functions below; keep their
//! signatures. Server calls go through crate::kit (kit::call, never on the
//! UI thread), the shared dialogs through kit::confirm / kit::prompt /
//! kit::menu. Pure rules go in their own files here, unit-tested.

use crate::{areas::AreaEvent, MainWindow};
use serde_json::{json, Value};

pub fn init(_w: &MainWindow) {}

pub fn signed_in() {}

pub fn signed_out() {}

pub fn project(_alias: &str) {}

pub fn tab_shown(_tab: &str) {}

pub fn event(_e: &AreaEvent) {}

/// The test driver's "desktop:<name>" actions; Some(answer) for a known one.
pub fn test_ui(name: &str, _args: &[String]) -> Option<Value> {
    match name {
        "ping" => Some(json!({ "ok": true, "area": "desktop" })),
        _ => None,
    }
}

/// What the test driver sees of the area ({"cmd":"area","area":"desktop"}).
pub fn props() -> Value {
    json!({})
}
