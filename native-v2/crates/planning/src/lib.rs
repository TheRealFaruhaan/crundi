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

pub mod factories;
pub mod kanban;
pub mod media;
pub mod mindmap;
pub mod notes;
pub mod paint;
pub mod schedule;
pub mod util;

use crundi_core::live::Event;
use crundi_shell::{Area, Stores};
use gpui::*;
use serde_json::{json, Value};

pub fn init(cx: &mut App) {
    kanban::init(cx);
    mindmap::init(cx);
    notes::init(cx);
    factories::init(cx);
    crundi_shell::register(cx, Area::new("planning")
        .tab("kanban", |stores, window, cx| kanban::tab(stores, window, cx))
        .tab("mindmap", |stores, window, cx| mindmap::tab(stores, window, cx))
        .tab("schedule", |stores, window, cx| schedule::tab(stores, window, cx))
        .tab("notes", |stores, window, cx| notes::tab(stores, window, cx))
        .tab("factories", |stores, window, cx| factories::tab(stores, window, cx))
        .on_batch(|stores, batch: &[Event], cx| {
            kanban::on_batch(stores, batch, cx);
            mindmap::on_batch(stores, batch, cx);
            schedule::on_batch(stores, batch, cx);
            notes::on_batch(stores, batch, cx);
            factories::on_batch(stores, batch, cx);
        })
        .on_project(|stores, alias, cx| { kanban::on_project(stores, alias, cx); mindmap::on_project(stores, alias, cx); schedule::on_project(stores, alias, cx); notes::on_project(stores, alias, cx); factories::on_project(stores, alias, cx); })
        .on_signed_in(|stores, cx| { watch_tab(stores, cx); kanban::on_signed_in(stores, cx); mindmap::on_signed_in(stores, cx); schedule::on_signed_in(stores, cx); notes::on_signed_in(stores, cx); factories::on_signed_in(stores, cx); })
        .on_signed_out(|stores, cx| { kanban::on_signed_out(stores, cx); mindmap::on_signed_out(stores, cx); schedule::on_signed_out(stores, cx); notes::on_signed_out(stores, cx); factories::on_signed_out(stores, cx); })
        // The empty pane's chips (the workbench runs "wb-chip:<kind>").
        .action("wb-chip:kanban", |_, _, cx| place(cx, "panel:kanban"))
        .action("wb-chip:mindmap", |_, _, cx| place(cx, "panel:mindmap"))
        .action("wb-chip:notes", |_, _, cx| notes::add_pane(cx))
        .action("wb-chip:factory", |_, _, cx| place(cx, "panel:factory"))
        .test_ui(|stores, name, args, window, cx| test_ui(stores, name, args, window, cx))
        // Under "planningArea", the names native-gpui's suites read; the
        // window-wide prompt and menu too (the driver's state has "dialog").
        .state(|stores, window, cx| {
            let k = kanban::state(stores, window, cx);
            let m = mindmap::state(stores, window, cx);
            let sc = schedule::state(stores, window, cx);
            let nt = notes::state(stores, window, cx);
            let fx = factories::state(stores, window, cx);
            let fx_open = fx["factories"]["pointerModal"].as_bool().unwrap_or(false);
            let sched_open = sc.get("pointerModal").and_then(|v| v.as_bool()).unwrap_or(false);
            let ask = kanban::ask::props(cx);
            let modal = !ask.is_null() || !crundi_widgets::ask_props(cx).is_null() || m.get("dialogOpen").and_then(|v| v.as_bool()).unwrap_or(false) || sched_open || fx_open || !media::props(cx).is_null();
            json!({ "planningArea": { "kanban": k, "mindmap": m, "schedule": sc, "notes": nt, "factories": fx["factories"], "ask": ask, "pointerModal": modal,
                "dialog": crundi_widgets::ask_props(cx), "menu": crundi_widgets::menu_props(cx), "mediaModal": media::props(cx) } })
        }));
}

/// A screen coming into view loads again (what changed while it was not on
/// show: the web reloads a tab as it shows it): the Kanban and Mindmap tabs,
/// their panes on the workbench, the Schedule tab.
struct TabWatch(#[allow(dead_code)] Subscription);
impl Global for TabWatch {}

fn watch_tab(stores: &Stores, cx: &mut App) {
    if cx.has_global::<TabWatch>() { return; }
    let mut last = String::new();
    let sub = cx.observe(&stores.nav, move |nav, cx| {
        let tab = nav.read(cx).tab.clone();
        if tab == last { return; }
        last = tab.clone();
        let tab2 = tab.clone();
        let tab3 = tab.clone();
        cx.defer(move |cx| notes::on_tab(&tab3, cx));
        cx.defer(move |cx| factories::on_tab(&tab, cx));
        cx.defer(move |cx| match tab2.as_str() {
            "kanban" => kanban::load(cx),
            "mindmap" => mindmap::reload(cx),
            "schedule" => schedule::reload(cx),
            "workbench" => {
                if util::pane_shown(cx, "panel:kanban") { kanban::load(cx); }
                if util::pane_shown(cx, "panel:mindmap") { mindmap::reload(cx); }
            }
            _ => {}
        });
    });
    cx.set_global(TabWatch(sub));
}

/// Put a planning panel on the workbench (the empty pane's chip, the driver).
pub fn place(cx: &mut App, key: &str) {
    if let Some(wb) = crundi_workbench::workbench(cx) { let k = key.to_string(); wb.update(cx, |w, cx| w.place(&k, cx)); }
}

fn measure_layout(stream: &str, cr: Option<(&str, &str)>, cx: &mut App) -> bool {
    use crundi_core::mosaic::{Dir, Mosaic};
    let Some(wb) = crundi_workbench::workbench(cx) else { return false };
    let stream = stream.to_string();
    // The change request's five panes: the Kanban over a chat, the Mindmap
    // over a terminal, the streaming chat (focused) in the third column.
    if let Some((chat, term)) = cr {
        let (chat, term) = (chat.to_string(), term.to_string());
        wb.update(cx, |w, cx| {
            let mut m = Mosaic::preset("cols3", vec!["panel:kanban".into(), "panel:mindmap".into(), stream.clone()]);
            for (above, k) in [("panel:kanban", &chat), ("panel:mindmap", &term)] {
                if let Some(l) = m.leaf_of(above) { let n = m.split(l, Dir::Col); m.assign(n, Some(k.clone())); }
            }
            *w.mosaic() = m;
            w.focus_key(&stream, cx);
            cx.notify();
        });
        return true;
    }
    wb.update(cx, |w, cx| {
        let keys = w.keys();
        let chats: Vec<String> = keys.iter().filter(|k| k.starts_with("live:") && **k != stream).cloned().collect();
        let mut m = Mosaic::preset("cols3", vec!["panel:kanban".into(), "panel:mindmap".into(), stream.clone()]);
        let under = |m: &mut Mosaic, above: &str, key: Option<&String>, dir: Dir| {
            let (Some(k), Some(l)) = (key, m.leaf_of(above)) else { return };
            let n = m.split(l, dir);
            m.assign(n, Some(k.clone()));
        };
        under(&mut m, "panel:kanban", chats.first(), Dir::Col);
        under(&mut m, "panel:mindmap", chats.get(1), Dir::Col);
        under(&mut m, &stream, chats.get(2), Dir::Col);
        if let Some(c3) = chats.get(2).cloned() { under(&mut m, &c3, chats.get(3), Dir::Row); }
        for k in chats.iter().skip(4) { m.place(k); }
        *w.mosaic() = m;
        // The streaming chat has the focus (the driver's "send" goes to it).
        w.focus_key(&stream, cx);
        cx.notify();
    });
    true
}

fn test_ui(stores: &Stores, name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    match name {
        "ping" => return Some(json!({ "ok": true, "area": "planning" })),
        "open-pane" => { let k = format!("panel:{}", args.first().cloned().unwrap_or_default()); place(cx, &k); return Some(json!({ "ok": true })); }
        // The frame budget's layout (tools/measure-pc.mjs): every pane in
        // view, in three columns: the Kanban over a chat, the Mindmap over a
        // chat, the given chat (the one streaming, focused) over the others.
        "measure-layout" => {
            let a = |i: usize| args.get(i).map(|s| s.as_str()).unwrap_or("");
            // "measure-layout <stream> cr <chat> <terminal>": the CR's five panes.
            let cr = (a(1) == "cr").then(|| (a(2), a(3)));
            return Some(json!({ "ok": measure_layout(a(0), cr, cx) }));
        }
        _ => {}
    }
    if let Some(v) = media::test_ui(name, args, window, cx) { return Some(v); }
    kanban::test_ui(stores, name, args, window, cx).or_else(|| mindmap::test_ui(stores, name, args, window, cx)).or_else(|| schedule::test_ui(stores, name, args, window, cx)).or_else(|| notes::test_ui(stores, name, args, window, cx)).or_else(|| factories::test_ui(stores, name, args, window, cx))
}
