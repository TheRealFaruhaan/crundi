//! The planning area (AREAS.md): phase 4: Kanban, Mind map, Notes, Schedules, webhooks, Factories.
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

/// The test driver's "planning:<name>" actions; Some(answer) for a known one.
pub fn test_ui(name: &str, _args: &[String]) -> Option<Value> {
    match name {
        "ping" => Some(json!({ "ok": true, "area": "planning" })),
        _ => None,
    }
}

/// What the test driver sees of the area ({"cmd":"area","area":"planning"}).
pub fn props() -> Value {
    json!({})
}
