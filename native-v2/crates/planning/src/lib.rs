//! The planning area (AREAS.md; ARCHITECTURE.md): Kanban (and its card
//! dialog), the Mind map, Notes, Schedules and webhooks, Factories. One
//! module per screen; each keeps its data in its own store entity (a Global
//! holds only the store's handle, read in handlers, never in render), and
//! draws it with cached leaf views placed side by side by a layout body
//! (the tab's view, or the workbench pane's body).
//!
//! This file registers the area: the tabs, the pane kinds, the live
//! batch (each screen takes the events it cares about, once per batch), the
//! project and sign-in changes, the driver's actions and state.

pub mod kanban;
pub mod mindmap;
pub mod util;

use crundi_core::live::Event;
use crundi_shell::{Area, Stores};
use gpui::*;
use serde_json::{json, Value};

pub fn init(cx: &mut App) {
    kanban::init(cx);
    mindmap::init(cx);
    crundi_shell::register(cx, Area::new("planning")
        .tab("kanban", |stores, window, cx| kanban::tab(stores, window, cx))
        .tab("mindmap", |stores, window, cx| mindmap::tab(stores, window, cx))
        .on_batch(|stores, batch: &[Event], cx| {
            kanban::on_batch(stores, batch, cx);
            mindmap::on_batch(stores, batch, cx);
        })
        .on_project(|stores, alias, cx| { kanban::on_project(stores, alias, cx); mindmap::on_project(stores, alias, cx); })
        .on_signed_in(|stores, cx| { kanban::on_signed_in(stores, cx); mindmap::on_signed_in(stores, cx); })
        .on_signed_out(|stores, cx| { kanban::on_signed_out(stores, cx); mindmap::on_signed_out(stores, cx); })
        // The empty pane's chips (the workbench runs "wb-chip:<kind>").
        .action("wb-chip:kanban", |_, _, cx| place(cx, "panel:kanban"))
        .action("wb-chip:mindmap", |_, _, cx| place(cx, "panel:mindmap"))
        .test_ui(|stores, name, args, window, cx| test_ui(stores, name, args, window, cx))
        // Under "planningArea", the names native-gpui's suites read; the
        // window-wide prompt and menu too (the driver's state has "dialog").
        .state(|stores, window, cx| {
            let k = kanban::state(stores, window, cx);
            let m = mindmap::state(stores, window, cx);
            let ask = kanban::ask::props(cx);
            let modal = !ask.is_null() || !crundi_widgets::ask_props(cx).is_null() || m.get("dialogOpen").and_then(|v| v.as_bool()).unwrap_or(false);
            json!({ "planningArea": { "kanban": k, "mindmap": m, "ask": ask, "pointerModal": modal,
                "dialog": crundi_widgets::ask_props(cx), "menu": crundi_widgets::menu_props(cx) } })
        }));
}

/// Put a planning panel on the workbench (the empty pane's chip, the driver).
pub fn place(cx: &mut App, key: &str) {
    if let Some(wb) = crundi_workbench::workbench(cx) { let k = key.to_string(); wb.update(cx, |w, cx| w.place(&k, cx)); }
}

fn test_ui(stores: &Stores, name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    match name {
        "ping" => return Some(json!({ "ok": true, "area": "planning" })),
        "open-pane" => { let k = format!("panel:{}", args.first().cloned().unwrap_or_default()); place(cx, &k); return Some(json!({ "ok": true })); }
        _ => {}
    }
    kanban::test_ui(stores, name, args, window, cx).or_else(|| mindmap::test_ui(stores, name, args, window, cx))
}
