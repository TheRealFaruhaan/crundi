//! The operations area's screens (Operations: phase 5; AREAS.md). Each screen in its own file
//! here, registered through `init`: its tabs (and, later, workbench panel
//! kinds with crundi_workbench::register_kind), its live-link handlers and
//! its test-driver actions and props ("operations:<name>").
//!
//! Until the area builds them, each tab says so.

use crundi_shell::Area;
use crundi_widgets as w;
use gpui::*;
use serde_json::json;

pub fn init(cx: &mut App) {
    // The chat's log side (crundi-chat-log, Operations' 2.13-2.23): its driver
    // actions and props. Until the app calls crundi_chat_log::init itself
    // (shared/requests.md), operations does.
    crundi_chat_log::init(cx);
    crundi_shell::register(cx, Area::new("operations")
        .tabs(&["panels", "services", "docker", "homepool", "info", "secrets", "settings"], |tab, _, _| w::empty(format!("{} comes with phases 3 to 6 of the GPUI client.", crundi_shell::registry::tab_label(tab))).into_any_element())
        .test_ui(|name, _, _, _| match name {
            "ping" => Some(json!({ "ok": true, "area": "operations" })),
            _ => None,
        }));
}
