//! The workspace area's screens (Workspace: phase 3's Git, Files, Media and the workbench's panels; AREAS.md). Each screen in its own file
//! here, registered through `init`: its tabs (and, later, workbench panel
//! kinds with crundi_workbench::register_kind), its live-link handlers and
//! its test-driver actions and props ("workspace:<name>").
//!
//! Until the area builds them, each tab says so.

use crundi_shell::Area;
use crundi_widgets as w;
use gpui::*;
use serde_json::json;

pub fn init(cx: &mut App) {
    crundi_shell::register(cx, Area::new("workspace")
        .tabs(&["git", "files", "media"], |tab, _, _| w::empty(format!("{} comes with phases 3 to 6 of the GPUI client.", crundi_shell::registry::tab_label(tab))).into_any_element())
        .test_ui(|name, _, _, _| match name {
            "ping" => Some(json!({ "ok": true, "area": "workspace" })),
            _ => None,
        }));
}
