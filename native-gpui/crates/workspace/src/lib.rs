//! The workspace area's screens (Workspace: phase 3's Git, Files, Media and
//! the workbench's panels; AREAS.md). Each screen in its own file here,
//! registered through `init`: its tabs (and workbench panel kinds), its
//! live-link handlers and its test-driver actions and props ("workspace:<name>").
//!
//!   git.rs       3.3 Git: status, staging, discard, commit, pull, push
//!   diff.rs      the line diff, its chunks and patches (the Slint client's, unchanged)
//!   diffview.rs  the diff window with staging per chunk
//!   files.rs     3.4 Files: browse, search, the row menu, upload, download
//!   wins.rs      the floating file windows: the editor and the picture viewer
//!   media.rs     3.5 Media: the library, its cards and the lightbox

pub mod diff;
pub mod diffview;
pub mod files;
pub mod git;
pub mod media;
pub mod wins;

use crundi_shell::Area;
use crundi_widgets as w;
use gpui::*;
use serde_json::json;

/// A query-string value.
pub(crate) fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// Run with the window, after the current update (a server call's answer,
/// or code already inside the window's own update).
pub(crate) fn with_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App) + 'static) {
    cx.defer(move |cx| {
        let Some(win) = cx.active_window().or_else(|| cx.windows().first().copied()) else { return };
        let _ = win.update(cx, move |_, window, cx| f(window, cx));
    });
}

/// Save what a GET answers to a file the person picks (the system's save
/// dialog; the driver's "pick-next" in tests).
pub(crate) fn save_url(url: String, name: String, cx: &mut App) {
    let Some(api) = crundi_shell::api(cx) else { return };
    crundi_shell::run(cx, move || -> Option<Result<String, String>> {
        let path = crundi_core::driver::take_pick().or_else(|| rfd::FileDialog::new().set_title(format!("Save {name}")).set_file_name(&name).save_file())?;
        Some(match api.get_bytes(&url) {
            Ok((200, bytes)) => std::fs::write(&path, bytes).map(|_| name.clone()).map_err(|e| e.to_string()),
            Ok(_) => Err("Could not download".into()),
            Err(e) => Err(e.to_string()),
        })
    }, |r, cx| match r {
        Some(Ok(n)) => toast(cx, format!("Saved {n}"), w::Toast::Success),
        Some(Err(e)) => toast(cx, e, w::Toast::Error),
        None => {}
    });
}

/// A toast from anywhere.
pub(crate) fn toast(cx: &mut App, text: String, kind: w::Toast) { with_window(cx, move |window, cx| w::toast(window, cx, text, kind)); }

pub fn init(cx: &mut App) {
    wins::init(cx);
    // Git, Files and Media as workbench panels (the "+" menu, an empty pane's chips).
    use std::rc::Rc;
    crundi_workbench::register_kind(cx, crundi_workbench::PaneKind { prefix: "panel:git", render: Rc::new(|_, window, cx| git::ensure(window, cx).into_any_element()), title: Rc::new(|_, _| ("Git".into(), String::new())) });
    crundi_workbench::register_kind(cx, crundi_workbench::PaneKind { prefix: "panel:files", render: Rc::new(|_, window, cx| files::ensure(window, cx).into_any_element()), title: Rc::new(|_, _| ("Files".into(), String::new())) });
    crundi_workbench::register_kind(cx, crundi_workbench::PaneKind { prefix: "panel:media", render: Rc::new(|_, window, cx| media::ensure(window, cx).into_any_element()), title: Rc::new(|_, _| ("Media".into(), String::new())) });
    crundi_shell::register(cx, Area::new("workspace")
        .tabs(&["git", "files", "media"], |tab, window, cx| match tab {
            "git" => git::ensure(window, cx).into_any_element(),
            "files" => files::ensure(window, cx).into_any_element(),
            "media" => media::ensure(window, cx).into_any_element(),
            _ => w::empty(format!("{} comes with phases 3 to 6 of the GPUI client.", crundi_shell::registry::tab_label(tab))).into_any_element(),
        })
        .on_project(|_, cx| {
            if let Some(g) = git::view(cx) { g.update(cx, |g, cx| { g.clear(cx); g.load(cx); }); }
            if let Some(f) = files::view(cx) { f.update(cx, |f, cx| { f.clear(cx); f.load(None, cx); }); }
            if let Some(m) = media::view(cx) { m.update(cx, |m, cx| m.load(cx)); }
        })
        .on_tab(|tab, cx| if tab == "git" { if let Some(g) = git::view(cx) { g.update(cx, |g, cx| g.load(cx)); } })
        .test_ui(|name, a, window, cx| {
            let s = |i: usize| a.get(i).cloned().unwrap_or_default();
            if name == "ping" { return Some(json!({ "ok": true, "area": "workspace" })); }
            if let Some(op) = name.strip_prefix("git-") {
                let g = git::ensure(window, cx);
                g.update(cx, |g, cx| match op {
                    "refresh" => g.load(cx),
                    "stage" => g.stage(&s(0), cx),
                    "unstage" => g.unstage(&s(0), cx),
                    "stage-all" => g.stage_all(cx),
                    "unstage-all" => g.unstage_all(cx),
                    "discard" => g.discard(&s(0), window, cx),
                    "diff" => g.diff(&s(0), s(1) == "1" || s(1) == "true", cx),
                    "commit" => g.commit(&s(0), window, cx),
                    "push" => g.push(cx),
                    "pull" => g.pull(cx),
                    _ => {}
                });
                return Some(json!({ "ok": true }));
            }
            if let Some(op) = name.strip_prefix("files-") {
                let f = files::ensure(window, cx);
                f.update(cx, |f, cx| match op {
                    "nav" => f.load(Some(s(0)), cx),
                    "up" => f.up(cx),
                    "refresh" => f.load(None, cx),
                    "search" => f.search(&s(0), cx),
                    "open" => f.open(&s(0), cx),
                    "menu" => f.menu(&s(0), point(px(0.), px(0.)), cx),
                    "paste" => f.paste_here(cx),
                    "upload" => f.upload(cx),
                    "download" => files::download(&s(0), cx),
                    _ => {}
                });
                return Some(json!({ "ok": true }));
            }
            if let Some(op) = name.strip_prefix("media-") {
                let m = media::ensure(window, cx);
                m.update(cx, |m, cx| match op {
                    "upload" => m.upload(cx),
                    "search" => m.set_search(&s(0), window, cx),
                    "kind" => m.set_kind(&s(0), cx),
                    "scope" => m.set_scope(&s(0), cx),
                    "reload" => m.load(cx),
                    "open" => m.open(&s(0), cx),
                    "get" => m.get(&s(0), cx),
                    "copy" => m.copy(&s(0), cx),
                    "jump" => m.jump(&s(0), window, cx),
                    "delete" => m.delete(&s(0), window, cx),
                    _ => {}
                });
                return Some(json!({ "ok": true }));
            }
            match name {
                "open-path" => { let p = s(0); files::open_path("", &p, cx); return Some(json!({ "ok": true })); }
                "lb-close" => { if media::view(cx).is_some_and(|m| m.read(cx).lb.is_some()) { w::close_dialog(window, cx); } return Some(json!({ "ok": true })); }
                _ => {}
            }
            let id: u32 = s(0).parse().unwrap_or(0);
            match name {
                "diff-step" => { let d: i32 = s(1).parse().unwrap_or(0); diffview::find(cx, id)?.update(cx, |w, cx| w.step(d, cx)); }
                "diff-stage" => { let c: usize = s(1).parse().unwrap_or(0); diffview::find(cx, id)?.update(cx, |w, cx| w.stage(c, cx)); }
                "win-close" => {
                    if diffview::find(cx, id).is_some() { w::close_dialog(window, cx); }
                    else { let l = wins::layer(cx)?; if !l.read(cx).has(id) { return None; } l.update(cx, |l, cx| l.close(id, cx)); }
                }
                "win-edit" => { let t = s(1); wins::layer(cx)?.update(cx, |l, cx| l.set_text(id, &t, window, cx)); }
                "win-save" => { let t = s(1); wins::layer(cx)?.update(cx, |l, cx| { if !t.is_empty() { l.set_text(id, &t, window, cx); } l.save(id, cx) }); }
                "win-expand" => { wins::layer(cx)?.update(cx, |l, cx| l.expand(id, cx)); }
                "win-front" => { wins::layer(cx)?.update(cx, |l, cx| l.front(id, cx)); }
                "win-arrange" => { let m = s(0); wins::layer(cx)?.update(cx, |l, cx| l.arrange(&m, cx)); }
                // A drag by the title bar: from (x0, y0) to (x1, y1), dropped there.
                "win-drag" => {
                    let n = |i: usize| s(i).parse::<f32>().unwrap_or(0.);
                    wins::layer(cx)?.update(cx, |l, cx| { l.drag(id, 0, n(1), n(2), cx); l.drag(id, 1, n(3), n(4), cx); l.drag(id, 2, n(3), n(4), cx); });
                }
                _ => return None,
            }
            Some(json!({ "ok": true }))
        })
        .props(|_, cx| { let mut all_wins = match diffview::props(cx) { serde_json::Value::Array(a) => a, _ => vec![] }; all_wins.extend(wins::props(cx)); json!({ "areas": { "workspace": {
            "git": git::view(cx).map(|g| g.read(cx).props(cx)).unwrap_or(json!({ "state": "loading" })),
            "wins": all_wins,
            "files": files::view(cx).map(|f| f.read(cx).props(cx)).unwrap_or(json!({ "state": "loading", "rows": [] })),
            "copied": files::view(cx).map(|f| f.read(cx).copied.clone()).unwrap_or_default(),
            "media": media::view(cx).map(|m| m.read(cx).props()).unwrap_or(json!({ "state": "loading", "cards": [] })),
        } } }) }));
}
