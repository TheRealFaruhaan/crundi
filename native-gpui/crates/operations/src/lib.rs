//! The operations area's screens (Operations: phase 5; AREAS.md), one
//! module per screen, as the Slint client's src/ops/ (its specs:
//! native-client/specs/ops/<stem>.md):
//!
//! | module | tab | what |
//! |---|---|---|
//! | services | services | services, their forwards and exposure |
//! | docker | docker | containers, stacks, images, volumes, networks, forwards, system |
//! | homepool | homepool | networks, nodes, access keys, requests, activity |
//! | info | info | the project, this machine, maintenance, collaborators, server logs |
//! | vault | secrets | secrets (the unlock flow), access requests |
//! | settings | settings | every Settings section for the owner |
//! | skills | (settings) | Skills, Usage, Backup & restore; the sign-in restore |
//! | panels | panels | panels (widgets): the tab, panes, inline card, dock, chips |
//! | extras | — | Add project, the approvals button and panel, What's new |
//!
//! The test driver: "ops:<name>" actions and the `ops` area's props (one
//! key per screen), as the Slint client's ops area, so
//! native-client/test/operations.mjs runs unchanged.

pub mod services;
pub mod docker;
pub mod homepool;
pub mod info;
pub mod vault;
pub mod settings;
pub mod skills;
pub mod panels;
pub mod extras;

use crundi_shell::Area;
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Map, Value};

macro_rules! each { ($f:ident ( $($a:expr),* )) => { services::$f($($a),*); docker::$f($($a),*); homepool::$f($($a),*); info::$f($($a),*); vault::$f($($a),*); settings::$f($($a),*); skills::$f($($a),*); panels::$f($($a),*); extras::$f($($a),*); }; }

/// A tab's page: the screen that draws it, with the ask layer (confirm,
/// prompt, context menu) over the window.
fn page(tab: &str, window: &mut Window, cx: &mut App) -> AnyElement {
    let screen = None
        .or_else(|| services::view(tab, window, cx))
        .or_else(|| docker::view(tab, window, cx))
        .or_else(|| homepool::view(tab, window, cx))
        .or_else(|| info::view(tab, window, cx))
        .or_else(|| vault::view(tab, window, cx))
        .or_else(|| settings::view(tab, window, cx))
        .or_else(|| panels::view(tab, window, cx))
        .unwrap_or_else(|| w::empty(format!("{} comes with phases 3 to 6 of the GPUI client.", crundi_shell::registry::tab_label(tab))).into_any_element());
    div().size_full().flex().flex_col().min_h_0().child(screen).into_any_element()
}

pub fn init(cx: &mut App) {
    // The chat's log side (crundi-chat-log, Operations' 2.13-2.23): its driver
    // actions and props. Until the app calls crundi_chat_log::init itself
    // (shared/requests.md), operations does.
    crundi_chat_log::init(cx);
    // The top bar's usage badge and bars: Settings, its Usage section open.
    crundi_shell::register_action(cx, "open-usage", |cx| {
        settings::set_open(cx, "usage", true);
        if let Some(s) = crundi_shell::shell(cx) { s.update(cx, |s, cx| s.show_tab("settings", cx)); }
    });
    crundi_shell::register(cx, Area::new("operations")
        .tabs(&["panels", "services", "docker", "homepool", "info", "secrets", "settings"], page)
        .on_event(|e, cx| { each!(on_event(e, cx)); })
        .on_project(|a, cx| { each!(on_project(a, cx)); })
        .on_signed_in(|cx| { each!(on_signed_in(cx)); })
        .on_signed_out(|cx| { each!(on_signed_out(cx)); })
        .on_tab(|t, cx| { each!(on_tab(t, cx)); })
        .test_ui(|name, _, _, _| match name {
            "ping" => Some(json!({ "ok": true, "area": "operations" })),
            _ => None,
        }));
    // The driver's "ops" area (the Slint client's name): every screen's
    // actions ("ops:<name>") and props (one key per screen).
    crundi_shell::register(cx, Area::new("ops")
        .test_ui(|name, args, window, cx| {
            if name == "ping" { return Some(json!({ "ok": true, "area": "ops" })); }
            None.or_else(|| services::test_ui(name, args, window, cx))
                .or_else(|| docker::test_ui(name, args, window, cx))
                .or_else(|| homepool::test_ui(name, args, window, cx))
                .or_else(|| info::test_ui(name, args, window, cx))
                .or_else(|| vault::test_ui(name, args, window, cx))
                .or_else(|| settings::test_ui(name, args, window, cx))
                .or_else(|| skills::test_ui(name, args, window, cx))
                .or_else(|| panels::test_ui(name, args, window, cx))
                .or_else(|| extras::test_ui(name, args, window, cx))
        })
        .props(|window, cx| {
            let mut o = Map::new();
            o.insert("services".into(), services::props(window, cx));
            o.insert("docker".into(), docker::props(window, cx));
            o.insert("homepool".into(), homepool::props(window, cx));
            o.insert("info".into(), info::props(window, cx));
            o.insert("vault".into(), vault::props(window, cx));
            o.insert("settings".into(), settings::props(window, cx));
            o.insert("skills".into(), skills::props(window, cx));
            o.insert("panels".into(), panels::props(window, cx));
            o.insert("extras".into(), extras::props(window, cx));
            // Under areas.ops, the Slint driver's shape (top-level keys
            // would clash with the other parts' in the merged props).
            // "clipboard" at the top, as the Slint client's props (info's Copy check reads it).
            json!({ "areas": { "ops": Value::Object(o) }, "clipboard": cx.read_from_clipboard().and_then(|c| c.text()).unwrap_or_default() })
        }));
}
