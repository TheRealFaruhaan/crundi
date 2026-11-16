//! The workspace area's screens (Workspace: phase 3's Git, Files, Media and
//! the workbench's panels; AREAS.md). Each screen in its own file here,
//! registered through `init`: its tabs (and workbench panel kinds), its
//! live-link handlers and its test-driver actions and props ("workspace:<name>").
//!
//!   git.rs       3.3 Git: status, staging, discard, commit, pull, push
//!   diff.rs      the line diff, its chunks and patches (the Slint client's, unchanged)
//!   diffview.rs  the diff window with staging per chunk

pub mod diff;
pub mod diffview;
pub mod git;

use crundi_shell::Area;
use crundi_widgets as w;
use gpui::*;
use serde_json::json;

/// A query-string value.
pub(crate) fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// A toast from anywhere (a server call's answer has no window): through
/// the window it was asked in.
pub(crate) fn toast(win: Option<AnyWindowHandle>, text: String, kind: w::Toast, cx: &mut App) {
    if let Some(h) = win { let _ = cx.update_window(h, |_, window, cx| w::toast(window, cx, text, kind)); }
}

pub fn init(cx: &mut App) {
    crundi_shell::register(cx, Area::new("workspace")
        .tabs(&["git", "files", "media"], |tab, window, cx| match tab {
            "git" => git::ensure(window, cx).into_any_element(),
            _ => w::empty(format!("{} comes with phases 3 to 6 of the GPUI client.", crundi_shell::registry::tab_label(tab))).into_any_element(),
        })
        .on_project(|_, cx| if let Some(g) = git::view(cx) { g.update(cx, |g, cx| { g.clear(cx); g.load(cx); }); })
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
            let id: u32 = s(0).parse().unwrap_or(0);
            match name {
                "diff-step" => { let d: i32 = s(1).parse().unwrap_or(0); diffview::find(cx, id)?.update(cx, |w, cx| w.step(d, cx)); }
                "diff-stage" => { let c: usize = s(1).parse().unwrap_or(0); diffview::find(cx, id)?.update(cx, |w, cx| w.stage(c, cx)); }
                "win-close" => { diffview::find(cx, id)?; w::close_dialog(window, cx); }
                _ => return None,
            }
            Some(json!({ "ok": true }))
        })
        .props(|_, cx| json!({ "areas": { "workspace": {
            "git": git::view(cx).map(|g| g.read(cx).props(cx)).unwrap_or(json!({ "state": "loading" })),
            "wins": diffview::props(cx),
        } } })));
}
