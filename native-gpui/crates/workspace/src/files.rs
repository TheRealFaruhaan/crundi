//! Files (phase 3.4; the web's loadFiles, renderFilesPanel, openFileMenu
//! and the files* actions; the Slint client's src/workspace/files.rs):
//! browse by absolute paths (the server resolves them; the path bar can go
//! above the project), search the project ("N matches · gitignored files
//! excluded"), open a file in the editor or a picture in the viewer
//! (wins.rs), and the row menu: Open, Open externally / Reveal (when the
//! server runs here), Cut, Copy, Paste, Copy path, Download, Delete (asks
//! first); Upload into the folder on show.

use crate::{toast, wins};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::time::Duration;

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }
fn truthy(v: &Value) -> bool { v.as_bool().unwrap_or(!v.is_null()) }
fn err_of(v: &Value, fail: &str) -> String { let e = s(v, "error"); if e.is_empty() { fail.into() } else { e } }

/// A path's last part, whichever way its slashes lean.
pub fn base_name(p: &str) -> String { p.rsplit(['/', '\\']).next().unwrap_or("").to_string() }

/// A file's mark in the list (fileIcon): its short text, or an icon; its tone.
pub fn file_mark(name: &str) -> (&'static str, Option<IconName>) {
    let base = name.to_ascii_lowercase();
    let ext = base.rfind('.').map(|i| &base[i..]).unwrap_or("");
    if base == "dockerfile" || base.starts_with("dockerfile.") { return ("", Some(IconName::Container)); }
    if base == "package.json" || base == "package-lock.json" { return ("npm", None); }
    if base == ".env" || base.starts_with(".env.") { return ("", Some(IconName::Lock)); }
    match ext {
        ".js" | ".mjs" | ".cjs" => ("JS", None), ".ts" => ("TS", None), ".tsx" => ("TX", None), ".jsx" => ("JX", None),
        ".json" => ("{}", None), ".html" => ("◇", None), ".css" | ".scss" => ("#", None), ".py" => ("PY", None),
        ".go" => ("Go", None), ".rs" => ("RS", None), ".java" => ("JV", None), ".c" => ("C", None), ".cpp" => ("C+", None),
        ".h" => ("H", None), ".sh" => ("$_", None), ".ps1" => ("PS", None), ".sql" => ("DB", None), ".md" => ("M↓", None),
        ".yaml" | ".yml" => ("YML", None), ".xml" => ("XML", None), ".vue" => ("V", None), ".svelte" => ("S", None),
        ".svg" | ".png" | ".jpg" | ".jpeg" | ".gif" | ".webp" => ("", Some(IconName::Image)),
        ".zip" | ".tar" | ".gz" => ("", Some(IconName::Archive)),
        ".pdf" => ("", Some(IconName::FileText)), ".lock" => ("", Some(IconName::Lock)), ".log" => ("", Some(IconName::List)),
        _ => ("", Some(IconName::File)),
    }
}

/// The size column (formatFileSize): "" for none.
pub fn fmt_size(bytes: f64) -> String {
    if bytes <= 0.0 { return String::new(); }
    if bytes < 1024.0 { return format!("{} B", bytes as u64); }
    if bytes < 1024.0 * 1024.0 { return format!("{:.1} KB", bytes / 1024.0); }
    format!("{:.1} MB", bytes / (1024.0 * 1024.0))
}

/// The search's note, or its empty line.
pub fn search_note(n: usize, truncated: bool, gitignore: bool) -> (String, String) {
    if n == 0 { return (String::new(), format!("No matches{}", if gitignore { " (gitignored files are excluded)" } else { "" })); }
    (format!("{}{} match{}{}", n, if truncated { "+" } else { "" }, if n == 1 { "" } else { "es" }, if gitignore { " · gitignored files excluded" } else { "" }), String::new())
}

#[derive(Clone)]
struct Clip { op: String, project: String, paths: Vec<String> }

/// One row as drawn and listed.
#[derive(Clone)]
pub struct Row { pub path: String, pub name: String, pub dir: String, pub is_dir: bool, pub up: bool, pub size: String, pub mark: String, pub icon: Option<IconName>, pub cut: bool, pub selected: bool }

pub struct FilesView {
    pub path: String,
    parent: Option<String>,
    inside: bool,
    entries: Vec<Value>,
    crumbs: Vec<Value>,
    shell: Option<Value>,
    pub state: String,
    error: String,
    q: String,
    results: Option<Value>,
    search_gen: u64,
    selected: String,
    clip: Option<Clip>,
    pub copied: String,
    search: Entity<w::InputState>,
    _sub: Subscription,
}

pub struct FilesHandle(pub Entity<FilesView>);
impl Global for FilesHandle {}
pub fn view(cx: &App) -> Option<Entity<FilesView>> { cx.try_global::<FilesHandle>().map(|h| h.0.clone()) }

impl FilesView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| w::InputState::new(window, cx).placeholder("Search files & folders…"));
        let _sub = cx.subscribe_in(&search, window, |f, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::Change = ev { let q = st.read(cx).value().to_string(); f.search(&q, cx); });
        FilesView { path: String::new(), parent: None, inside: false, entries: vec![], crumbs: vec![], shell: None, state: "loading".into(), error: String::new(), q: String::new(), results: None, search_gen: 0, selected: String::new(), clip: None, copied: String::new(), search, _sub }
    }

    fn clip_here(&self, cx: &App) -> bool { self.clip.as_ref().is_some_and(|c| c.project == crundi_shell::project(cx) && !c.paths.is_empty()) }

    fn row(&self, e: &Value, dir: Option<String>) -> Row {
        let path = s(e, "path");
        let name = s(e, "name");
        let is_dir = s(e, "type") == "dir";
        let (mark, icon) = if is_dir { ("", Some(IconName::Folder)) } else { file_mark(&name) };
        Row {
            cut: self.clip.as_ref().is_some_and(|c| c.op == "cut" && c.paths.contains(&path)), selected: self.selected == path,
            size: if is_dir { String::new() } else { fmt_size(e.get("size").and_then(|v| v.as_f64()).unwrap_or(0.)) },
            path, name, dir: dir.unwrap_or_default(), is_dir, up: false, mark: mark.into(), icon,
        }
    }

    /// The rows on show (the folder, or the search's results) and the note / empty line.
    pub fn rows(&self) -> (Vec<Row>, String, String) {
        let mut rows = vec![];
        let (mut note, mut empty) = (String::new(), String::new());
        if let Some(r) = &self.results {
            if let Some(e) = r.get("error").and_then(|e| e.as_str()) { empty = format!("Search error: {e}"); } else {
                let list = r.get("entries").and_then(|e| e.as_array()).cloned().unwrap_or_default();
                (note, empty) = search_note(list.len(), r.get("truncated").is_some_and(truthy), r.get("gitignoreApplied").is_some_and(truthy));
                for e in &list { let rel = s(e, "rel"); rows.push(self.row(e, rel.rfind('/').map(|i| rel[..=i].to_string()))); }
            }
        } else {
            if let Some(p) = &self.parent { rows.push(Row { path: p.clone(), name: "..".into(), dir: String::new(), is_dir: true, up: true, size: String::new(), mark: String::new(), icon: Some(IconName::ArrowUp), cut: false, selected: false }); }
            for e in &self.entries { rows.push(self.row(e, None)); }
        }
        (rows, note, empty)
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.path.clear(); self.parent = None; self.inside = false; self.entries.clear(); self.crumbs.clear(); self.state = "loading".into(); self.q.clear(); self.results = None; self.selected.clear();
        cx.notify();
    }

    /// GET /api/files/list (dir: an absolute path; none: the one on show, or
    /// the project). A folder that went falls back, with a toast.
    pub fn load(&mut self, dir: Option<String>, cx: &mut Context<Self>) {
        let project = crundi_shell::project(cx);
        if project.is_empty() { return cx.notify(); }
        let target = dir.unwrap_or_else(|| self.path.clone());
        let path = format!("/api/files/list?project={}&dir={}", crate::enc(&project), crate::enc(&target));
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
            if crundi_shell::project(cx) != project { return; }
            let mut fell = None;
            let _ = me.update(cx, |f, cx| {
                match r {
                    Ok(v) if v.get("ok").is_some_and(truthy) => {
                        if v.get("fellBack").is_some_and(truthy) { fell = Some(s(&v, "path")); }
                        f.path = s(&v, "path");
                        f.parent = v.get("parent").and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(String::from);
                        f.inside = v.get("inside").is_some_and(truthy);
                        f.entries = v.get("entries").and_then(|e| e.as_array()).cloned().unwrap_or_default();
                        f.crumbs = v.get("crumbs").and_then(|e| e.as_array()).cloned().unwrap_or_default();
                        f.shell = v.get("localShell").filter(|x| x.is_object()).cloned();
                        f.state = "ok".into();
                        f.q.clear();
                        f.results = None;
                    }
                    Ok(v) => { f.state = "error".into(); f.error = err_of(&v, "Failed"); }
                    Err(e) => { f.state = "error".into(); f.error = format!("Error: {e}"); }
                }
                cx.notify();
            });
            if let Some(p) = fell { let b = base_name(&p); toast(cx, format!("That folder no longer exists. Showing {} instead.", if b.is_empty() { p } else { b }), w::Toast::Info); }
        });
    }

    pub fn up(&mut self, cx: &mut Context<Self>) { if let Some(p) = self.parent.clone() { self.load(Some(p), cx); } }

    /// Typing in the search box: the results 220ms after the last key.
    pub fn search(&mut self, q: &str, cx: &mut Context<Self>) {
        self.q = q.to_string();
        self.search_gen += 1;
        let g = self.search_gen;
        if q.trim().is_empty() { self.results = None; return cx.notify(); }
        cx.spawn(async move |this: WeakEntity<FilesView>, cx: &mut AsyncApp| {
            cx.background_executor().timer(Duration::from_millis(220)).await;
            let _ = this.update(cx, |f, cx| {
                if f.search_gen != g { return; }
                let q = f.q.trim().to_string();
                let path = format!("/api/files/search?project={}&q={}", crate::enc(&crundi_shell::project(cx)), crate::enc(&q));
                let me = cx.entity().downgrade();
                crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
                    let _ = me.update(cx, |f, cx| {
                        if f.q.trim() != q { return; }
                        f.results = Some(match r {
                            Ok(v) if v.get("ok").is_some_and(truthy) => v,
                            Ok(v) => json!({ "entries": [], "error": s(&v, "error") }),
                            Err(e) => json!({ "entries": [], "error": e }),
                        });
                        cx.notify();
                    });
                });
            });
        }).detach();
    }

    fn is_dir(&self, path: &str) -> bool {
        if self.parent.as_deref() == Some(path) { return true; }
        let list = self.results.as_ref().and_then(|r| r.get("entries")).and_then(|e| e.as_array()).cloned().unwrap_or_default();
        self.entries.iter().chain(list.iter()).any(|e| s(e, "path") == path && s(e, "type") == "dir")
    }

    /// Double click: a folder opens here, a file in a window.
    pub fn open(&mut self, path: &str, cx: &mut Context<Self>) {
        if self.is_dir(path) { return self.load(Some(path.to_string()), cx); }
        wins::open_file(&crundi_shell::project(cx), path, cx);
    }

    /// The row menu (openFileMenu), at a window point.
    pub fn menu(&mut self, path: &str, at: Point<Pixels>, cx: &mut Context<Self>) {
        if self.parent.as_deref() == Some(path) && self.results.is_none() { return; }
        self.selected = path.to_string();
        let dir = self.is_dir(path);
        let name = base_name(path);
        let clip = self.clip.clone().filter(|_| self.clip_here(cx));
        let here = self.path.clone();
        let mut items = vec![w::MenuEntry::new("open", "Open").icon(if dir { IconName::Folder } else { IconName::File })];
        if let Some(sh) = &self.shell {
            let fm = s(sh, "fileManager");
            if !dir { items.push(w::MenuEntry::new("ext", "Open externally").icon(IconName::ExternalLink).sep()); }
            let it = w::MenuEntry::new(if dir { "shell-open" } else { "reveal" }, if dir { format!("Open in {fm}") } else { format!("Reveal in {fm}") }).icon(IconName::Folder);
            items.push(if dir { it.sep() } else { it });
        }
        items.push(w::MenuEntry::new("cut", "Cut").icon(IconName::Scissors).sep());
        items.push(w::MenuEntry::new("copy", "Copy").icon(IconName::Copy));
        if let Some(c) = &clip {
            let what = if c.paths.len() == 1 { String::new() } else { format!(" {} items", c.paths.len()) };
            if dir { items.push(w::MenuEntry::new("paste-into", format!("Paste{what} inside \u{201c}{name}\u{201d}")).icon(IconName::Clipboard)); }
            if self.inside { items.push(w::MenuEntry::new("paste-here", format!("Paste{what}{}", if dir { " in the current folder" } else { " here" })).icon(IconName::Clipboard)); }
        }
        items.push(w::MenuEntry::new("path", "Copy path").icon(IconName::Copy).sep());
        if !dir { items.push(w::MenuEntry::new("download", "Download").icon(IconName::Download)); }
        items.push(w::MenuEntry::new("delete", "Delete").icon(IconName::Trash).danger().sep());
        let p = path.to_string();
        let me = cx.entity().downgrade();
        w::menu_at(cx, at, items, move |id, window, cx| {
            let p = p.clone();
            let here = here.clone();
            let _ = me.update(cx, |f, cx| match id.as_str() {
                "open" => f.open(&p, cx),
                "ext" | "shell-open" => f.shell_act("open", &p, cx),
                "reveal" => f.shell_act("reveal", &p, cx),
                "cut" => f.set_clip("cut", &p, cx),
                "copy" => f.set_clip("copy", &p, cx),
                "paste-into" => f.paste(&p, cx),
                "paste-here" => f.paste(&here, cx),
                "path" => f.copy_path(&p, cx),
                "download" => download(&p, cx),
                "delete" => f.delete(&p, window, cx),
                _ => {}
            });
        });
        cx.notify();
    }

    fn shell_act(&mut self, act: &str, p: &str, cx: &mut Context<Self>) {
        let body = json!({ "project": crundi_shell::project(cx), "file": p, "act": act });
        crundi_shell::call(cx, move |api| api.post("/api/files/shell", &body), |r, cx| match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => toast(cx, err_of(&v, "Could not do that"), w::Toast::Error),
            Err(e) => toast(cx, e, w::Toast::Error),
            _ => {}
        });
    }

    fn set_clip(&mut self, op: &str, p: &str, cx: &mut Context<Self>) {
        self.clip = Some(Clip { op: op.into(), project: crundi_shell::project(cx), paths: vec![p.to_string()] });
        toast(cx, format!("{}{} Paste it into a folder.", base_name(p), if op == "cut" { " is ready to move." } else { " is ready to copy." }), w::Toast::Info);
        cx.notify();
    }

    pub fn paste_here(&mut self, cx: &mut Context<Self>) { let here = self.path.clone(); self.paste(&here, cx); }

    /// Paste once into a folder: /api/files/transfer.
    fn paste(&mut self, to: &str, cx: &mut Context<Self>) {
        if !self.clip_here(cx) { return; }
        let Some(c) = self.clip.take() else { return };
        let op = if c.op == "cut" { "move" } else { "copy" };
        let body = json!({ "project": crundi_shell::project(cx), "op": op, "items": c.paths, "to": to });
        cx.notify();
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.post("/api/files/transfer", &body), move |r, cx| {
            match r {
                Ok(d) => {
                    let did = d.get("done").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                    let bad = d.get("failed").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                    if let Some(first) = did.first() {
                        let what = if did.len() == 1 { s(first, "name") } else { format!("{} items", did.len()) };
                        let renamed = did.len() == 1 && first.get("renamed").is_some_and(truthy);
                        toast(cx, format!("{}{}{}", if op == "move" { "Moved " } else { "Copied " }, what, if renamed { " (renamed: that name was taken)" } else { "" }), w::Toast::Success);
                    }
                    if !bad.is_empty() || d.get("ok").and_then(|o| o.as_bool()) == Some(false) {
                        let msg = bad.first().map(|b| format!("{}: {}", base_name(&s(b, "from")), s(b, "error"))).unwrap_or_else(|| err_of(&d, &format!("Could not {op}")));
                        toast(cx, msg, w::Toast::Error);
                    }
                }
                Err(e) => toast(cx, e, w::Toast::Error),
            }
            let _ = me.update(cx, |f, cx| f.load(None, cx));
        });
    }

    fn copy_path(&mut self, p: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(p.to_string()));
        self.copied = p.to_string();
        cx.set_global(crate::Copied(p.to_string()));
        toast(cx, "Path copied".into(), w::Toast::Info);
    }

    /// Delete asks first; the folder on show reloads.
    fn delete(&mut self, p: &str, window: &mut Window, cx: &mut Context<Self>) {
        let name = base_name(p);
        let p = p.to_string();
        let me = cx.entity().downgrade();
        let c = w::Confirm { title: "Delete".into(), message: format!("Delete \"{name}\"?\n\nThis permanently removes it from disk and cannot be undone.").into(), ok: "Delete".into(), danger: true };
        w::confirm(window, cx, c, move |_, cx| {
            let body = json!({ "project": crundi_shell::project(cx), "file": p });
            let (me, name) = (me.clone(), name.clone());
            crundi_shell::call(cx, move |api| api.post("/api/files/delete", &body), move |r, cx| {
                match r {
                    Ok(v) if v.get("ok").is_some_and(truthy) => toast(cx, format!("Deleted {name}"), w::Toast::Success),
                    Ok(v) => toast(cx, format!("Delete failed: {}", err_of(&v, "unknown")), w::Toast::Error),
                    Err(e) => toast(cx, format!("Delete failed: {e}"), w::Toast::Error),
                }
                let _ = me.update(cx, |f, cx| f.load(None, cx));
            });
        });
    }

    /// Upload: the system's picker, each file into the folder on show
    /// (/api/files/upload, base64), a toast each, then the list again.
    pub fn upload(&mut self, cx: &mut Context<Self>) {
        let dir = self.path.clone();
        self.upload_into(dir, None, cx);
    }

    /// A row's path from the project's root (the web's absProjectPath).
    fn abs_path(&self, p: &str, cx: &App) -> String {
        if p.starts_with('/') || p.contains(":\\") { return p.to_string(); }
        let project = crundi_shell::project(cx);
        let root = crundi_shell::session::get(cx).projects.iter().find(|x| x.get("alias").and_then(|a| a.as_str()) == Some(project.as_str()))
            .and_then(|x| x.get("path").and_then(|v| v.as_str())).unwrap_or("").trim_end_matches('/').to_string();
        if root.is_empty() { p.to_string() } else if p.is_empty() { root } else { format!("{root}/{}", p.trim_start_matches('/')) }
    }

    /// Files dropped from the system: into `dir` (the folder on show, or the
    /// folder row they were dropped on), as Upload does (the owner, 18 Nov:
    /// no drag and drop in Files).
    pub fn drop_into(&mut self, dir: String, paths: Vec<std::path::PathBuf>, cx: &mut Context<Self>) {
        let files: Vec<_> = paths.into_iter().filter(|p| p.is_file()).collect();
        if files.is_empty() { return toast(cx, "Only files can be dropped here (not folders).".into(), w::Toast::Error); }
        self.upload_into(dir, Some(files), cx);
    }

    /// Upload `given` (else the system's picker) into `dir`.
    fn upload_into(&mut self, dir: String, given: Option<Vec<std::path::PathBuf>>, cx: &mut Context<Self>) {
        let project = crundi_shell::project(cx);
        if project.is_empty() { return; }
        let Some(api) = crundi_shell::api(cx) else { return };
        let me = cx.entity().downgrade();
        crundi_shell::run(cx, move || {
            let paths = given.unwrap_or_else(|| crundi_core::driver::take_pick().map(|p| vec![p]).or_else(|| rfd::FileDialog::new().pick_files()).unwrap_or_default());
            paths.into_iter().map(|p| {
                use base64::Engine;
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
                let r = std::fs::read(&p).map_err(|e| e.to_string()).and_then(|bytes| api.post("/api/files/upload", &json!({ "project": project, "dir": dir, "name": name, "data": base64::engine::general_purpose::STANDARD.encode(bytes) })).map_err(|e| e.to_string()));
                (name, r)
            }).collect::<Vec<_>>()
        }, move |done, cx| {
            for (name, r) in done {
                match r {
                    Ok(v) if v.get("ok").is_some_and(truthy) => toast(cx, format!("Uploaded {name}"), w::Toast::Success),
                    Ok(v) => toast(cx, format!("Upload failed: {}", err_of(&v, "unknown")), w::Toast::Error),
                    Err(e) => toast(cx, format!("Error: {e}"), w::Toast::Error),
                }
            }
            let _ = me.update(cx, |f, cx| f.load(None, cx));
        });
    }

    pub fn props(&self, cx: &App) -> Value {
        let (rows, note, empty) = self.rows();
        json!({
            "state": if crundi_shell::project(cx).is_empty() { "none".to_string() } else { self.state.clone() },
            "path": self.path, "inside": self.inside, "parent": self.parent, "searching": self.results.is_some(), "selected": self.selected,
            "note": note, "empty": empty,
            "clip": self.clip.as_ref().map(|c| json!({ "op": c.op, "paths": c.paths })),
            "rows": rows.iter().map(|r| json!({ "name": r.name, "dir": r.dir, "path": r.path, "isDir": r.is_dir, "size": r.size, "mark": r.mark, "cut": r.cut, "selected": r.selected })).collect::<Vec<_>>(),
            "crumbs": self.crumbs.iter().map(|c| s(c, "name")).collect::<Vec<_>>(),
        })
    }

    fn line(&self, r: &Row, i: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let (p1, p2, p3) = (r.path.clone(), r.path.clone(), r.path.clone());
        let mark: AnyElement = match (&r.icon, r.mark.is_empty()) {
            (Some(icon), true) => div().text_color(if r.is_dir { T::amber() } else { T::text_secondary() }).child(w::icon(icon.clone(), T::ICON_MD)).into_any_element(),
            _ => div().text_size(T::TEXT_10).font_weight(T::W_BOLD).font_family(T::MONO).text_color(T::accent_hover()).child(r.mark.clone()).into_any_element(),
        };
        let abs = self.abs_path(&r.path, cx);
        let (drop_dir, is_folder) = (r.path.clone(), r.is_dir && !r.up);
        let row = div().id(("file", i)).group("file-row").flex().items_center().gap(T::SP_8).h(T::CONTROL_SM).px(T::SP_10).cursor_pointer();
        // Dragged into a chat or a terminal: its path, as the web's [File Path: …].
        let row = if r.up { row } else { w::drag_ref(row, if r.is_dir { "folder" } else { "file" }, &abs, r.name.clone()) };
        // A folder takes files dropped from the system: into it.
        let row = if is_folder {
            row.drag_over::<ExternalPaths>(|s, _, _, _| s.bg(T::accent_dim()))
                .on_drop(cx.listener(move |f, p: &ExternalPaths, _, cx| { cx.stop_propagation(); f.drop_into(drop_dir.clone(), p.paths().to_vec(), cx); }))
        } else { row };
        row
            .when(r.selected, |d| d.bg(T::bg_tertiary())).hover(|s| s.bg(T::bg_hover()))
            .when(r.cut, |d| d.opacity(T::WS_O_CUT))
            .on_click(cx.listener(move |f, e: &ClickEvent, _, cx| { if e.click_count() >= 2 { f.open(&p1, cx) } else { f.selected = p1.clone(); cx.notify(); } }))
            .on_mouse_down(MouseButton::Right, cx.listener(move |f, e: &MouseDownEvent, _, cx| f.menu(&p2, e.position, cx)))
            .child(div().flex_none().w(T::ICON_LG).flex().justify_center().child(mark))
            .child(div().flex().flex_1().min_w_0().items_center().text_size(T::TEXT_13)
                .when(!r.dir.is_empty(), |d| d.child(div().text_color(T::text_muted()).truncate().child(r.dir.clone())))
                .child(div().text_color(T::text()).truncate().child(r.name.clone())))
            .child(div().flex_none().text_size(T::TEXT_11).font_family(T::MONO).text_color(T::text_muted()).child(r.size.clone()))
            .when(!r.up, |d| d.child(w::icon_button(("file-more", i), IconName::Ellipsis, "More").on_click(cx.listener(move |f, e: &ClickEvent, _, cx| f.menu(&p3, e.position(), cx)))))
    }
}

/// Download: where to save first, then the file.
pub fn download(p: &str, cx: &mut App) {
    let name = { let b = base_name(p); if b.is_empty() { "download".to_string() } else { b } };
    let url = format!("/api/files/download?project={}&file={}", crate::enc(&crundi_shell::project(cx)), crate::enc(p));
    crate::save_url(url, name, cx);
}

/// A path in a chat's reply (openPathFromChat): inside a project, its
/// editor; anything else is resolved by the server first. A trailing
/// :line or :line:col is dropped.
pub fn open_path(project: &str, raw: &str, cx: &mut App) {
    let mut p = raw.trim().trim_start_matches("./").to_string();
    for _ in 0..2 { if let Some(i) = p.rfind(':') { if i > 0 && i + 1 < p.len() && p[i + 1..].chars().all(|c| c.is_ascii_digit()) { p.truncate(i); } } }
    if p.is_empty() { return; }
    if p.starts_with('/') {
        let owner = crundi_shell::session::get(cx).projects.iter()
            .filter(|x| { let r = s(x, "path"); let r = r.trim_end_matches('/'); !r.is_empty() && (p == r || p.starts_with(&format!("{r}/"))) })
            .max_by_key(|x| s(x, "path").len()).cloned();
        if let Some(o) = owner { return wins::open_file(&s(&o, "alias"), &p, cx); }
    }
    let proj = if project.is_empty() { crundi_shell::project(cx) } else { project.to_string() };
    let path = format!("/api/files/resolve?project={}&path={}", crate::enc(&proj), crate::enc(&p));
    crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| match r {
        Ok(d) if d.get("ok").is_some_and(truthy) => {
            if d.get("dir").is_some_and(truthy) { return toast(cx, format!("That is a folder: {p}"), w::Toast::Info); }
            wins::open_file(&proj, &s(&d, "file"), cx);
        }
        Ok(_) => toast(cx, format!("File not found: {p}"), w::Toast::Error),
        Err(_) => wins::open_file(&proj, &p, cx),
    });
}

impl Render for FilesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (rows, note, empty) = self.rows();
        let mut crumbs = div().flex().flex_1().min_w_0().items_center().gap(T::SP_4).overflow_hidden();
        for (i, c) in self.crumbs.clone().iter().enumerate() {
            if i > 0 { crumbs = crumbs.child(div().text_color(T::text_muted()).child("/")); }
            let p = s(c, "path");
            crumbs = crumbs.child(div().id(("crumb", i)).text_size(T::TEXT_12).text_color(T::accent_hover()).cursor_pointer().hover(|s| s.text_color(T::text()))
                .on_click(cx.listener(move |f, _, _, cx| f.load(Some(p.clone()), cx))).child(s(c, "name")));
        }
        let paste = self.inside && self.clip_here(cx);
        let bar = w::toolbar().child(crumbs)
            .when(paste, |d| d.child(w::icon_button("files-paste", IconName::Clipboard, "Paste into this folder").on_click(cx.listener(|f, _, _, cx| f.paste_here(cx)))))
            .when(self.parent.is_some(), |d| d.child(w::icon_button("files-up", IconName::ArrowUp, "Up").on_click(cx.listener(|f, _, _, cx| f.up(cx)))))
            .child(w::icon_button("files-upload", IconName::Upload, "Upload").on_click(cx.listener(|f, _, _, cx| f.upload(cx))));
        let body: AnyElement = match self.state.as_str() {
            _ if crundi_shell::project(cx).is_empty() => w::empty("Select a project").into_any_element(),
            "loading" => w::loading("Loading…").into_any_element(),
            "error" => w::error_state("files-retry", self.error.clone(), cx.listener(|f, _, _, cx| f.load(None, cx))).into_any_element(),
            _ => {
                let mut list = div().id("files-list").flex().flex_col().flex_1().min_h_0().overflow_y_scroll();
                if !note.is_empty() { list = list.child(div().px(T::SP_10).py(T::SP_4).text_size(T::TEXT_11).text_color(T::text_muted()).child(note)); }
                if rows.is_empty() { list = list.child(w::empty(if empty.is_empty() { "Empty folder".to_string() } else { empty })); }
                for (i, r) in rows.iter().enumerate() { list = list.child(self.line(r, i, cx)); }
                list.into_any_element()
            }
        };
        let here = self.path.clone();
        div().id("files-view").flex().flex_col().size_full().bg(T::bg())
            // Files from the system, dropped anywhere here: into the folder on show.
            .drag_over::<ExternalPaths>(|s, _, _, _| s.border_2().border_dashed().border_color(T::accent()))
            .on_drop(cx.listener(move |f, p: &ExternalPaths, _, cx| f.drop_into(here.clone(), p.paths().to_vec(), cx)))
            .child(div().p(T::SP_8).border_b_1().border_color(T::border()).child(w::input(&self.search)))
            .child(bar)
            .child(body)
    }
}

pub fn ensure(window: &mut Window, cx: &mut App) -> Entity<FilesView> {
    if let Some(v) = view(cx) { return v; }
    let v = cx.new(|cx| FilesView::new(window, cx));
    cx.set_global(FilesHandle(v.clone()));
    v.update(cx, |f, cx| f.load(None, cx));
    v
}

#[cfg(test)]
mod tests {
    use super::{base_name, file_mark, fmt_size, search_note};

    #[test]
    fn marks_and_sizes() {
        assert_eq!(file_mark("app.js").0, "JS");
        assert_eq!(file_mark("README.md").0, "M↓");
        assert!(file_mark("x.unknown").1.is_some());
        assert_eq!(fmt_size(0.0), "");
        assert_eq!(fmt_size(1.0), "1 B");
        assert_eq!(fmt_size(2048.0), "2.0 KB");
        assert_eq!(base_name("/a/b/c.txt"), "c.txt");
        assert_eq!(base_name("C:\\x\\y.md"), "y.md");
    }

    #[test]
    fn notes() {
        assert_eq!(search_note(3, false, true), ("3 matches · gitignored files excluded".into(), String::new()));
        assert_eq!(search_note(1, true, false), ("1+ match".into(), String::new()));
        assert_eq!(search_note(0, false, true), (String::new(), "No matches (gitignored files are excluded)".into()));
    }
}
