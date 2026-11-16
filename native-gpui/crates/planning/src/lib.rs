//! The planning area's screens (Planning: phase 4; AREAS.md), one module per screen:
//! kanban (4.1, 4.2), mindmap (4.3), notes (4.4), schedule (4.5, 4.6), factories (4.7, 4.8).
//! This file registers the part: each tab goes to its module's view; live events,
//! project and sign-in changes go to every module; the driver's "planning:<name>"
//! actions to the first module that knows them; the props are every module's,
//! merged, under `planningArea` (and `areas.planning` once the driver builds that
//! map: shared/requests.md).

pub mod factories;
pub mod kanban;
pub mod mindmap;
pub mod notes;
pub mod schedule;

use crundi_shell::Area;
use gpui::*;
use serde_json::{json, Map, Value};

macro_rules! each { ($f:ident ( $($a:expr),* )) => { kanban::$f($($a),*); mindmap::$f($($a),*); notes::$f($($a),*); schedule::$f($($a),*); factories::$f($($a),*); } }

pub fn init(cx: &mut App) {
    // The chat composer's part (Planning's, phase 2). The app doesn't call it yet
    // (shared/requests.md); planning registers it meanwhile.
    crundi_chat_composer::init(cx);
    each!(init(cx));
    crundi_shell::register(cx, Area::new("planning")
        .tabs(&["kanban", "notes", "mindmap", "factories", "schedule"], |tab, window, cx| match tab {
            "kanban" => kanban::view(tab, "", window, cx),
            "mindmap" => mindmap::view(tab, "", window, cx),
            "notes" => notes::view(tab, "", window, cx),
            "schedule" => schedule::view(tab, "", window, cx),
            _ => factories::view(tab, "", window, cx),
        })
        .on_event(|e, cx| { each!(on_event(e, cx)); })
        .on_project(|a, cx| { each!(on_project(a, cx)); })
        .on_signed_in(|cx| { each!(on_signed_in(cx)); })
        .on_signed_out(|cx| { each!(on_signed_out(cx)); })
        .on_tab(|t, cx| { each!(on_tab(t, cx)); })
        .test_ui(|name, args, window, cx| match name {
            "ping" => Some(json!({ "ok": true, "area": "planning" })),
            _ => kanban::test_ui(name, args, window, cx)
                .or_else(|| mindmap::test_ui(name, args, window, cx))
                .or_else(|| notes::test_ui(name, args, window, cx))
                .or_else(|| schedule::test_ui(name, args, window, cx))
                .or_else(|| factories::test_ui(name, args, window, cx)),
        })
        .props(|window, cx| {
            let mut m = Map::new();
            for v in [kanban::props(window, cx), mindmap::props(window, cx), notes::props(window, cx), schedule::props(window, cx), factories::props(window, cx)] {
                if let Value::Object(o) = v { m.extend(o); }
            }
            json!({ "planningArea": Value::Object(m) })
        }));
}
