//! The workspace area (AREAS.md; ARCHITECTURE.md's rules): the Claude
//! terminal panes, Git, Files (and the floating file windows) and Media, as
//! tabs and as workbench panels. Each screen in its own file, registered
//! through `init`: its tabs and pane kinds, the live hub's frame (terminal
//! output), the launcher's terminal buttons, the empty pane's chips, and the
//! test driver's actions and state ("workspace:<name>").
//!
//!   terms.rs     3.2 the Claude terminal panes (term:<id>): state, screen, cursor, input bar
//!   vt.rs        their terminal emulator (native-gpui's, unchanged)
//!   git.rs       3.3 Git: status, staging, discard, commit, pull, push
//!   diff.rs      the line diff, its chunks and patches (native-gpui's, unchanged)
//!   diffview.rs  the diff (a file window) with staging per chunk
//!   frames.rs    pages shown live (a PDF, a video, audio): the Desktop area's browser hook
//!   files.rs     3.4 Files: browse, search, the row menu, upload, download
//!   wins.rs      the floating file windows: the editor, the viewer, the diff; the Arrange bar
//!   media.rs     3.5 Media: the library, its cards and the lightbox

pub mod diff;
pub mod diffview;
pub mod files;
pub mod frames;
pub mod git;
pub mod media;
pub mod terms;
pub mod vt;
pub mod wins;

use crundi_core::live::Event;
use crundi_shell::Area;
use crundi_theme::IconName;
use crundi_widgets as w;
use crundi_workbench::{self as wb, HeadInfo, PaneHead, PaneKind, PaneParts};
use gpui::*;
use serde_json::{json, Value};
use std::rc::Rc;

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

/// What the area last put on the clipboard (the driver's "copied": the
/// test machine may have no clipboard).
#[derive(Default)]
pub(crate) struct Copied(pub(crate) String);
impl Global for Copied {}

pub(crate) fn note_copy(cx: &mut App, text: String) { cx.set_global(Copied(text)); }

/// A toast from anywhere.
pub(crate) fn toast(cx: &mut App, text: String, kind: w::Toast) { with_window(cx, move |window, cx| w::toast(window, cx, text, kind)); }

/// A tab's root (layout, not cached by the shell): the screen's view placed
/// cached, so a redraw of the shell never redraws the screen (rule 3).
struct Tab(AnyView);
impl Render for Tab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.0.clone().cached(StyleRefinement::default().size_full()))
    }
}

/// A panel kind (Git, Files, Media in a pane): its view, cached, as the body.
fn panel_kind(prefix: &'static str, title: &'static str, make: fn(&mut Window, &mut App) -> AnyView) -> PaneKind {
    PaneKind {
        prefix,
        chat: false,
        make: Rc::new(move |_, _, window, cx| {
            let v = make(window, cx);
            PaneParts {
                body: Rc::new(move |_| v.clone().cached(StyleRefinement::default().size_full()).into_any_element()),
                info: Rc::new(move |_| HeadInfo { title: title.into(), long: String::new(), state: String::new() }),
                watch: Box::new(|_| vec![]),
            }
        }),
        close: None,
        zoom: None,
    }
}

/// A panel's Refresh (refreshWbCell): its list again.
fn refresh(prefix: &str, cx: &mut App) {
    match prefix {
        "panel:git" => if let Some(g) = git::view(cx) { g.update(cx, |g, cx| g.load(cx)); },
        "panel:files" => if let Some(f) = files::view(cx) { f.update(cx, |f, cx| f.load(None, cx)); },
        _ => if let Some(m) = media::view(cx) { m.update(cx, |m, cx| m.load(cx)); },
    }
}

fn git_view(window: &mut Window, cx: &mut App) -> AnyView { git::ensure(window, cx).into() }
fn files_view(window: &mut Window, cx: &mut App) -> AnyView { files::ensure(window, cx).into() }
fn media_view(window: &mut Window, cx: &mut App) -> AnyView { media::ensure(window, cx).into() }

/// An empty pane's chip (the web's emptyLeafHtml): a panel in the
/// workbench (one of each: a second says so), or a new terminal launcher.
fn chip(kind: &str, cx: &mut App) {
    let Some(w) = wb::workbench(cx) else { return };
    if kind == "terminal" {
        w.update(cx, |w, cx| {
            let n = w.keys().iter().filter_map(|k| k.strip_prefix("pend:")?.parse::<u32>().ok()).max().unwrap_or(0) + 1;
            w.place(&format!("pend:{n}"), cx);
        });
        return;
    }
    let key = format!("panel:{kind}");
    let label = match kind { "git" => "Git", "files" => "Files", _ => "Media" };
    if w.update(cx, |w, _| w.keys().contains(&key)) { return toast(cx, format!("{label} is already in the workbench"), w::Toast::Info); }
    w.update(cx, |w, cx| { w.place(&key, cx); w.focus_key(&key, cx); });
}

pub fn init(cx: &mut App) {
    wins::init(cx);
    media::init(cx);
    wb::register_kind(cx, panel_kind("panel:git", "Git", git_view));
    wb::register_kind(cx, panel_kind("panel:files", "Files", files_view));
    wb::register_kind(cx, panel_kind("panel:media", "Media", media_view));
    // The panels' heads as the web's headHtmlPanel: the kind's icon, Refresh.
    for (prefix, icon) in [("panel:git", IconName::GitBranch), ("panel:files", IconName::Folder), ("panel:media", IconName::Images)] {
        wb::register_head(cx, prefix, wb::KindHead::new().icon(icon)
            .button(wb::HeadButton::icon("refresh", IconName::RefreshCw, "Refresh"))
            .on_action(move |_, action, _, cx| if action == "refresh" { refresh(prefix, cx); }));
    }
    // The Claude terminal panes (3.2): a chat's head (dot, A- ↺ A+, the
    // two-click ×), the screen and its input bar.
    wb::register_kind(cx, PaneKind {
        prefix: "term:",
        chat: true,
        make: Rc::new(|key, stores, window, cx| {
            let id = terms::id_of(key);
            let parts = terms::ensure(&id, window, cx);
            let (s2, id2, projects) = (stores.clone(), id.clone(), stores.projects.clone());
            PaneParts {
                body: Rc::new(move |cx| parts.body(cx)),
                info: Rc::new(move |cx| terms::head_info(&s2, &id2, cx)),
                // The head redraws on the event stream's list (its title and
                // state), never on its output.
                watch: Box::new(move |cx| vec![cx.observe(&projects, |h: &mut PaneHead, _, cx| h.refresh(cx))]),
            }
        }),
        close: Some(Rc::new(|key, cx, done| terms::close(key, cx, done))),
        zoom: Some(Rc::new(|key, d, cx| if let Some(p) = terms::find(cx, &terms::id_of(key)) { p.state.update(cx, |s, cx| s.set_zoom(d, cx)); })),
    });
    // The launcher's Empty Shell, Terminal and its Skip Permissions.
    wb::launcher::set_terminal_launcher(cx, Rc::new(|key, mode, _, cx| terms::launch(key, mode, cx)));
    let mut area = Area::new("workspace")
        .tab("git", |_, window, cx| { let v = git_view(window, cx); cx.new(|_| Tab(v)).into() })
        .tab("files", |_, window, cx| { let v = files_view(window, cx); cx.new(|_| Tab(v)).into() })
        .tab("media", |_, window, cx| { let v = media_view(window, cx); cx.new(|_| Tab(v)).into() })
        .on_batch(|stores, batch, cx| {
            terms::route(batch, cx);
            if batch.iter().any(|e| matches!(e, Event::Connected(true))) { terms::resubscribe(cx); }
            if batch.iter().any(|e| matches!(e, Event::State(_))) { terms::sync(stores, cx); }
        })
        .on_project(|stores, _, cx| {
            if let Some(g) = git::view(cx) { g.update(cx, |g, cx| { g.clear(cx); g.load(cx); }); }
            if let Some(f) = files::view(cx) { f.update(cx, |f, cx| { f.clear(cx); f.load(None, cx); }); }
            if let Some(m) = media::view(cx) { m.update(cx, |m, cx| m.load(cx)); }
            terms::clear(cx);
            terms::sync(stores, cx);
        })
        .on_signed_out(|_, cx| terms::clear(cx))
        // A path clicked in a chat: in the file viewer (the web's openPathFromChat).
        .action("open-path", |_, _, cx| {
            let p = cx.try_global::<crundi_shell::OpenPath>().map(|o| o.0.clone()).unwrap_or_default();
            files::open_path("", &p, cx);
        })
        .test_ui(|stores, name, a, window, cx| test_ui(stores, name, a, window, cx))
        .state(|_, window, cx| state(window, cx));
    for k in ["git", "files", "media", "terminal"] {
        area = area.action(match k { "git" => "wb-chip:git", "files" => "wb-chip:files", "media" => "wb-chip:media", _ => "wb-chip:terminal" }, move |_, _, cx| chip(k, cx));
    }
    crundi_shell::register(cx, area);
}

/// The driver's actions ("workspace:<name>").
fn test_ui(stores: &crundi_shell::Stores, name: &str, a: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    let s = |i: usize| a.get(i).cloned().unwrap_or_default();
    let ok = Some(json!({ "ok": true }));
    match name {
        "ping" => return Some(json!({ "ok": true, "area": "workspace" })),
        // The file the next picker answers with (the system's dialog can't be driven).
        "pick-next" => { crundi_core::driver::set_pick(Some(s(0).into())); return ok; }
        // An empty pane's chip, as clicked (git, files, media, terminal).
        "chip" => { chip(&s(0), cx); return ok; }
        // A launcher's Empty Shell / Terminal / Skip Permissions: its pane key
        // (else the first launcher), mode shell | normal | skip.
        "launch" => {
            let key = if s(0).starts_with("pend:") { s(0) } else { wb::workbench(cx)?.update(cx, |w, _| w.keys().into_iter().find(|k| k.starts_with("pend:")))? };
            let mode = s(1);
            terms::launch(&key, &mode, cx);
            return ok;
        }
        "open-path" => { files::open_path("", &s(0), cx); return ok; }
        "lb-close" => { if let Some(m) = media::view(cx) { m.update(cx, |m, cx| if m.lb.is_some() { m.close_lightbox(cx) }); } return ok; }
        // A panel's Refresh in its pane head (git, files, media).
        "panel-refresh" => { refresh(&format!("panel:{}", s(0)), cx); return ok; }
        _ => {}
    }
    if name.starts_with("term-") { return terms::act(name, a, window, cx); }
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
        return ok;
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
            "source" => wins::open_source(&crundi_shell::project(cx), &s(0), cx),
            // A row dropped on a folder (from, to, the folder's name): the Move / Copy menu.
            "drop-on" => f.drop_on_folder(&s(0), &s(1), &s(2), point(px(200.), px(200.)), cx),
            "escape" => { f.search_escape(window, cx); }
            _ => {}
        });
        return ok;
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
        return ok;
    }
    let _ = stores;
    let id: u32 = s(0).parse().unwrap_or(0);
    match name {
        "diff-step" => { let d: i32 = s(1).parse().unwrap_or(0); diffview::find(cx, id)?.update(cx, |w, cx| w.step(d, cx)); }
        "diff-stage" => { let c: usize = s(1).parse().unwrap_or(0); diffview::find(cx, id)?.update(cx, |w, cx| w.stage(c, cx)); }
        "win-close" => { let l = wins::layer(cx)?; if !l.read(cx).has(id) { return None; } l.update(cx, |l, cx| l.close(id, cx)); }
        "win-edit" => { let t = s(1); wins::layer(cx)?.update(cx, |l, cx| l.set_text(id, &t, window, cx)); }
        "win-save" => { let t = s(1); wins::layer(cx)?.update(cx, |l, cx| { if !t.is_empty() { l.set_text(id, &t, window, cx); } l.save(id, cx) }); }
        "win-expand" => { wins::layer(cx)?.update(cx, |l, cx| l.expand(id, cx)); }
        "win-front" => { wins::layer(cx)?.update(cx, |l, cx| l.front(id, cx)); }
        "win-arrange" => { let m = s(0); wins::layer(cx)?.update(cx, |l, cx| l.arrange(&m, cx)); }
        // A resize by a handle (e, s or c): from (x0, y0) to (x1, y1).
        "win-resize" => {
            let n = |i: usize| s(i).parse::<f32>().unwrap_or(0.);
            let d = s(1).chars().next().unwrap_or('c');
            wins::layer(cx)?.update(cx, |l, cx| { l.size(id, d, 0, n(2), n(3), cx); l.size(id, d, 1, n(4), n(5), cx); l.size(id, d, 2, n(4), n(5), cx); });
        }
        // A drag by the title bar: from (x0, y0) to (x1, y1), dropped there.
        "win-drag" => {
            let n = |i: usize| s(i).parse::<f32>().unwrap_or(0.);
            wins::layer(cx)?.update(cx, |l, cx| { l.drag(id, 0, n(1), n(2), cx); l.drag(id, 1, n(3), n(4), cx); l.drag(id, 2, n(3), n(4), cx); });
        }
        _ => return None,
    }
    ok
}

/// The area's part of the driver's state ("workspace").
fn state(window: &mut Window, cx: &mut App) -> Value {
    let mut all_wins = diffview::props(cx);
    all_wins.extend(wins::props(cx));
    let vs = f32::from(window.viewport_size().width);
    json!({ "workspace": {
        "git": git::view(cx).map(|g| g.read(cx).props(cx)).unwrap_or(json!({ "state": "loading" })),
        "files": files::view(cx).map(|f| f.read(cx).props(cx)).unwrap_or(json!({ "state": "loading", "rows": [] })),
        "media": media::view(cx).map(|m| m.read(cx).props()).unwrap_or(json!({ "state": "loading", "cards": [] })),
        "wins": all_wins,
        "terms": terms::props(cx),
        "copied": cx.try_global::<Copied>().map(|c| c.0.clone()).unwrap_or_default(),
        "menu": w::menu_props(cx),
        "pointerModal": w::modal_open(cx),
        "windowW": vs,
    } })
}
