//! The operations area (AREAS.md; ARCHITECTURE.md), ported from
//! native-gpui's operations crate, one module per screen (the web's
//! src/webapp-html.js is the reference; native-gpui's specs under
//! native-gpui/specs/ops/<stem>.md):
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
//! On v2's rules: each screen keeps its data in its own entity (a Global
//! holds only its handle, read in handlers); the tab's view is a layout view
//! (`Tab`) that places the screen's view cached; a screen notifies only when
//! its own state changed; the live batch is handled once per frame.
//!
//! The test driver: "ops:<name>" actions and the `ops` area's state (one key
//! per screen, under `areas.ops`), as native-gpui's, so its suites port as
//! they are (native-v2/test/operations.mjs).

pub mod compat;
pub mod ptt;
pub mod web;
pub mod services;
pub mod docker;
pub mod homepool;
pub mod info;
pub mod vault;
pub mod settings;
pub mod skills;
pub mod panels;
pub mod extras;

use crundi_shell::{Area, Stores};
use gpui::*;
use serde_json::{json, Map, Value};

pub use compat::{register_settings_part, settings_part_changed};

macro_rules! each { ($f:ident ( $($a:expr),* )) => { services::$f($($a),*); docker::$f($($a),*); homepool::$f($($a),*); info::$f($($a),*); vault::$f($($a),*); settings::$f($($a),*); skills::$f($($a),*); panels::$f($($a),*); extras::$f($($a),*); }; }

/// The operations tabs, in the web's order.
pub const TABS: [&str; 7] = ["panels", "services", "docker", "homepool", "info", "secrets", "settings"];

/// A tab's view: layout only, its screen placed as a cached leaf
/// (ARCHITECTURE.md rule 3).
pub struct Tab(AnyView);
impl Render for Tab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("ops-tab");
        div().size_full().flex().flex_col().min_h_0().child(self.0.clone().cached(StyleRefinement::default().size_full()))
    }
}

fn screen(tab: &str, window: &mut Window, cx: &mut App) -> AnyView {
    None.or_else(|| services::view(tab, window, cx))
        .or_else(|| docker::view(tab, window, cx))
        .or_else(|| homepool::view(tab, window, cx))
        .or_else(|| info::view(tab, window, cx))
        .or_else(|| vault::view(tab, window, cx))
        .or_else(|| settings::view(tab, window, cx))
        .or_else(|| panels::view(tab, window, cx))
        .expect("an operations tab")
}

fn tab(id: &'static str, window: &mut Window, cx: &mut App) -> AnyView {
    let s = screen(id, window, cx);
    // Made as its tab first shows: it hears that it is on show.
    each!(on_tab(id, cx));
    cx.new(|_| Tab(s)).into()
}

/// A screen coming into view, or going (each screen's on_tab): the nav's
/// tab watched once a sign-in.
struct TabWatch(#[allow(dead_code)] Subscription);
impl Global for TabWatch {}

fn watch_tab(stores: &Stores, cx: &mut App) {
    if cx.has_global::<TabWatch>() { return; }
    let mut last = stores.nav.read(cx).tab.clone();
    let sub = cx.observe(&stores.nav, move |nav, cx| {
        let tab = nav.read(cx).tab.clone();
        if tab == last { return; }
        last = tab.clone();
        cx.defer(move |cx| { each!(on_tab(&tab, cx)); });
    });
    cx.set_global(TabWatch(sub));
}

pub fn init(cx: &mut App) {
    let mut area = Area::new("operations");
    for t in TABS { area = area.tab(t, move |_, window, cx| tab(t, window, cx)); }
    crundi_shell::register(cx, area
        .on_batch(|_, batch, cx| {
            for e in compat::events(batch) { each!(on_event(&e, cx)); }
        })
        .on_project(|_, a, cx| { each!(on_project(a, cx)); })
        .on_signed_in(|stores, cx| { watch_tab(stores, cx); each!(on_signed_in(cx)); })
        .on_signed_out(|_, cx| { each!(on_signed_out(cx)); })
        // The top bar's usage badge and bars: Settings, its Usage section open.
        .action("open-usage", |_, _, cx| { settings::set_open(cx, "usage", true); compat::show_tab(cx, "settings"); })
        // The sidebar's "+ Add project".
        .action("add-project", |_, window, cx| extras::open_add_project(window, cx))
        // The empty pane's chip for a panel (the web's "widget" chip).
        .action("wb-chip:widget", |_, _, cx| panels::place_pane(cx))
        .test_ui(|_, name, args, window, cx| match name {
            "ping" => Some(json!({ "ok": true, "area": "operations" })),
            _ => test_ui(name, args, window, cx),
        }));
    // The driver's "ops" area (native-gpui's name): every screen's actions
    // ("ops:<name>") and state (one key per screen, under areas.ops).
    crundi_shell::register(cx, Area::new("ops")
        .test_ui(|_, name, args, window, cx| {
            if name == "ping" { return Some(json!({ "ok": true, "area": "ops" })); }
            test_ui(name, args, window, cx)
        })
        .state(|_, window, cx| {
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
            // "clipboard" at the top, as native-gpui's (info's Copy check reads it).
            json!({ "areas": { "ops": Value::Object(o) }, "clipboard": cx.read_from_clipboard().and_then(|c| c.text()).unwrap_or_default() })
        }));
    panels::init(cx);
    extras::init(cx);
    skills::init(cx);
    vault::register_badge(cx);
    settings::register_badge(cx);
}

fn test_ui(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    None.or_else(|| services::test_ui(name, args, window, cx))
        .or_else(|| docker::test_ui(name, args, window, cx))
        .or_else(|| homepool::test_ui(name, args, window, cx))
        .or_else(|| info::test_ui(name, args, window, cx))
        .or_else(|| vault::test_ui(name, args, window, cx))
        .or_else(|| settings::test_ui(name, args, window, cx))
        .or_else(|| skills::test_ui(name, args, window, cx))
        .or_else(|| panels::test_ui(name, args, window, cx))
        .or_else(|| extras::test_ui(name, args, window, cx))
}
