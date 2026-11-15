//! The workspace area (AREAS.md): phase 3's workbench panels, Git, Files, Media,
//! the file windows (editor, viewer, diff) and the Claude terminal panels.
//!
//! The registry (src/areas.rs) calls the functions below; keep their
//! signatures. Server calls go through crate::kit (kit::call, never on the
//! UI thread), the shared dialogs through kit::confirm / kit::prompt /
//! kit::menu. Pure rules go in their own files here, unit-tested.

pub mod diff;
pub mod files;
pub mod git;
pub mod hl;
pub mod media;
pub mod terms;
pub mod vt;
pub mod wins;

use crate::{areas::AreaEvent, kit, MainWindow};
use serde_json::{json, Value};
use slint::ComponentHandle;
use std::cell::RefCell;

thread_local! {
    /// The tab on show and the project, as the area last heard.
    static AT: RefCell<(String, String)> = RefCell::new(("workbench".into(), String::new()));
    /// The theme's colours the Rust side draws with (highlighting, the
    /// terminal's palette), read once at start.
    static PAL: RefCell<Palette> = RefCell::new(Palette::default());
}

/// Colours from the theme, as hex (for StyledText markup) and as colours.
#[derive(Default, Clone)]
pub struct Palette {
    pub hl: Vec<(hl::Tok, String)>,
    pub term: Vec<slint::Color>,
    pub fg: slint::Color,
    pub bg: slint::Color,
    pub diff_del: String,
    pub diff_add: String,
    pub text: String,
    pub white: String,
}

pub fn pal() -> Palette { PAL.with(|p| p.borrow().clone()) }

fn hex(c: slint::Color) -> String { format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue()) }

/// The workspace's global, for a moment.
pub fn g<R>(f: impl FnOnce(&crate::WorkspaceUi) -> R) -> Option<R> {
    let w = kit::ui()?;
    let r = f(&w.global::<crate::WorkspaceUi>());
    Some(r)
}

thread_local! { static COPIED: RefCell<String> = const { RefCell::new(String::new()) }; }

/// What the area last put on the clipboard (the driver reads it: the test
/// machine may have no clipboard).
pub fn note_copy(t: &str) { COPIED.with(|c| *c.borrow_mut() = t.to_string()); }

pub fn tab() -> String { AT.with(|a| a.borrow().0.clone()) }

/// A kind on show: its tab, or a pane of it in the workbench on show.
pub fn shown(kind: &str) -> bool {
    let t = tab();
    t == kind || (t == "workbench" && crate::app::wb_kinds().iter().any(|k| k == kind))
}

pub fn init(w: &MainWindow) {
    let th = w.global::<crate::Theme>();
    use hl::Tok::*;
    let p = Palette {
        hl: vec![
            (Plain, hex(th.get_ws_hl_plain())), (Keyword, hex(th.get_ws_hl_keyword())), (Str, hex(th.get_ws_hl_string())),
            (Comment, hex(th.get_ws_hl_comment())), (Number, hex(th.get_ws_hl_number())), (Func, hex(th.get_ws_hl_func())),
            (Type, hex(th.get_ws_hl_type())), (Prop, hex(th.get_ws_hl_prop())), (Op, hex(th.get_ws_hl_op())),
            (Tag, hex(th.get_ws_hl_prop())), (Attr, hex(th.get_ws_hl_number())), (Heading, hex(th.get_ws_hl_func())),
        ],
        // TERM_THEME: black, red, green, yellow, blue, magenta, cyan, white, then the bright ones.
        term: vec![
            th.get_bg_tertiary(), th.get_red(), th.get_green(), th.get_yellow(), th.get_accent(), th.get_ws_term_magenta(), th.get_ws_term_cyan(), th.get_text_primary(),
            th.get_text_muted(), th.get_ws_diff_del_text(), th.get_ws_diff_add_text(), th.get_amber(), th.get_accent_hover(), th.get_ws_term_magenta_hi(), th.get_ws_term_cyan_hi(), th.get_white(),
        ],
        fg: th.get_text_primary(),
        bg: th.get_bg_primary(),
        diff_del: hex(th.get_ws_diff_del_text()),
        diff_add: hex(th.get_ws_diff_add_text()),
        text: hex(th.get_text_primary()),
        white: hex(th.get_white()),
    };
    PAL.with(|x| *x.borrow_mut() = p);
    git::init(w);
    files::init(w);
    media::init(w);
    wins::init(w);
    terms::init(w);
    let ws = w.global::<crate::WorkspaceUi>();
    ws.on_launch(|leaf, mode| crate::app::launch_term(leaf as u32, mode.as_str()));
    ws.on_panel_refresh(|kind, _key| refresh_panel(kind.as_str()));
    ws.set_ready(true);
}

pub fn signed_in() {}

pub fn signed_out() {
    git::clear();
    files::clear();
    media::clear();
    wins::close_all();
    terms::clear();
}

pub fn project(alias: &str) {
    AT.with(|a| a.borrow_mut().1 = alias.to_string());
    git::clear();
    files::clear();
    media::project_changed();
    // (The terminals follow the panes: app.rs has already synced them. The
    // file windows stay: each holds its own project, as the web's.)
    if alias.is_empty() { return; }
    // What is on show loads now; the rest when it shows.
    for k in ["git", "files", "media"] { if shown(k) { panel_opened(k); } }
}

pub fn tab_shown(t: &str) {
    AT.with(|a| a.borrow_mut().0 = t.to_string());
    match t {
        "git" | "files" | "media" => panel_opened(t),
        "workbench" => { for k in crate::app::wb_kinds() { panel_opened(&k); } }
        _ => {}
    }
    git::timer();
}

/// A Git, Files or Media view came on show (its tab, or its pane added):
/// load it (the web's switchTab / wbLoad).
pub fn panel_opened(kind: &str) {
    match kind {
        "git" => git::load(),
        "files" => files::load(None),
        "media" => media::load(),
        _ => {}
    }
    git::timer();
}

/// A panel pane's Refresh (wbLoad): ours load again; another area's panel
/// is told as when its tab shows (it loads on that).
pub fn refresh_panel(kind: &str) {
    match kind {
        "git" | "files" | "media" => panel_opened(kind),
        k => {
            crate::planning::tab_shown(k);
            crate::ops::tab_shown(k);
            crate::desktop::tab_shown(k);
        }
    }
}

pub fn event(e: &AreaEvent) {
    match e {
        AreaEvent::Connected(true) => terms::resubscribe(),
        AreaEvent::Ws("output", v) => terms::output(&kit::s(v, "id"), &kit::s(v, "data")),
        _ => {}
    }
}

/// The app's terminal panes now (3.2): the ones to keep live.
pub fn terms_sync(ids: &[String]) { terms::sync(ids); }

/// A path in a chat's reply (2.16's path chips): opens in the editor, as
/// the web's openPathFromChat.
pub fn open_path(raw: &str) { files::open_path_from_chat(&kit::project(), raw); }

/// The test driver's "workspace:<name>" actions; Some(answer) for a known one.
pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let ok = Some(json!({ "ok": true }));
    match name {
        "ping" => Some(json!({ "ok": true, "area": "workspace" })),
        // Git.
        "git-refresh" => { git::load(); ok }
        "git-stage" => { git::stage(&a(0)); ok }
        "git-unstage" => { git::unstage(&a(0)); ok }
        "git-stage-all" => { git::stage_all(); ok }
        "git-unstage-all" => { git::unstage_all(); ok }
        "git-discard" => { git::discard(&a(0)); ok }
        "git-diff" => { git::diff(&a(0), a(1) == "1" || a(1) == "true"); ok }
        "git-commit" => { git::commit(&a(0)); ok }
        "git-push" => { git::push(); ok }
        "git-pull" => { git::pull(); ok }
        // Files.
        "files-nav" => { files::load(Some(a(0))); ok }
        "files-up" => { files::up(); ok }
        "files-search" => { files::search(&a(0)); ok }
        "files-select" => { files::select(&a(0)); ok }
        "files-open" => { files::open(&a(0)); ok }
        "files-menu" => { files::menu(&a(0), 200.0, 200.0); ok }
        "files-upload" => { files::upload(); ok }
        "files-paste" => { files::paste_here(); ok }
        "files-download" => { files::download(&a(0)); ok }
        "open-path" => { open_path(&a(0)); ok }
        // Media.
        "media-kind" => { media::set_kind(&a(0)); ok }
        "media-scope" => { media::set_scope(&a(0)); ok }
        "media-search" => { media::search(&a(0)); ok }
        "media-upload" => { media::upload(); ok }
        "media-open" => { media::open(&a(0)); ok }
        "media-get" => { media::get(&a(0)); ok }
        "media-copy" => { media::copy(&a(0)); ok }
        "media-jump" => { media::jump(&a(0)); ok }
        "media-delete" => { media::delete(&a(0)); ok }
        "lb-close" => { media::lb_close(); ok }
        "lb-play" => { media::lb_play(); ok }
        // Windows.
        "win-save" => { wins::save(a(0).parse().unwrap_or(0), &a(1)); ok }
        "win-edit" => { wins::edited(a(0).parse().unwrap_or(0), &a(1)); ok }
        "win-close" => { wins::close(a(0).parse().unwrap_or(0)); ok }
        "win-expand" => { wins::expand(a(0).parse().unwrap_or(0)); ok }
        "win-arrange" => { wins::arrange(&a(0)); ok }
        "win-drag" => {
            let id = a(0).parse().unwrap_or(0);
            let f = |i: usize| a(i).parse::<f32>().unwrap_or(0.0);
            wins::drag(id, 0, f(1), f(2)); wins::drag(id, 1, f(3), f(4)); wins::drag(id, 2, f(3), f(4));
            ok
        }
        "diff-stage" => { wins::diff_stage(a(0).parse().unwrap_or(0), a(1).parse().unwrap_or(0)); ok }
        "diff-step" => { wins::diff_step(a(0).parse().unwrap_or(0), a(1).parse().unwrap_or(1)); ok }
        // Terminals.
        "term-send" => { terms::send_text(&a(0), &a(1)); ok }
        "term-input" => { terms::input(&a(0), &a(1)); ok }
        "term-tool" => { terms::tool(&a(0), &a(1)); ok }
        "term-tools" => { terms::toggle_tools(&a(0)); ok }
        "term-attach" => { terms::attach(&a(0)); ok }
        "term-select" => { let n = |i: usize| a(i).parse::<i32>().unwrap_or(0); terms::select(&a(0), n(1), n(2), n(3), n(4)); ok }
        "term-scroll" => { terms::scroll(&a(0), a(1).parse().unwrap_or(0)); ok }
        "term-key" => Some(json!({ "ok": terms::key(&a(0), &a(1), a(2) == "ctrl", a(2) == "alt", a(2) == "shift") })),
        "launch" => { crate::app::launch_term(a(0).parse().unwrap_or(0), &a(1)); ok }
        _ => None,
    }
}

/// What the test driver sees of the area ({"cmd":"area","area":"workspace"}).
pub fn props() -> Value {
    json!({
        "git": git::props(),
        "files": files::props(),
        "media": media::props(),
        "wins": wins::props(),
        "terms": terms::props(),
        "copied": COPIED.with(|c| c.borrow().clone()),
    })
}
