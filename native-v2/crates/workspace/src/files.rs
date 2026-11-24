//! Files (phase 3.4; the web's loadFiles, renderFilesPanel, openFileMenu
//! and the files* actions; the Slint client's src/workspace/files.rs):
//! browse by absolute paths (the server resolves them; the path bar can go
//! above the project), search the project ("N matches · gitignored files
//! excluded"), open a file in the editor or a picture in the viewer
//! (wins.rs), and the row menu: Open, Open externally / Reveal (when the
//! server runs here), New file, Cut, Copy, Paste, Copy path, Download,
//! Delete (asks first); Upload into the folder on show. New file is also on
//! the list's empty space and the bar's "+".
//!
//! The server's answer (a folder, a search) becomes rows once, when it
//! arrives (`derive`), never while drawing; the list draws only the rows on
//! screen (`uniform_list`: every row is a control's height).

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

/// A file's mark in the list (fileIcon): its short text, or an icon; its
/// colour's name (T::ws_ft), none for the muted default.
pub fn file_mark(name: &str) -> (&'static str, Option<IconName>, Option<&'static str>) {
    let base = name.to_ascii_lowercase();
    let ext = base.rfind('.').map(|i| &base[i..]).unwrap_or("");
    if base == "dockerfile" || base.starts_with("dockerfile.") { return ("", Some(IconName::Container), Some("docker")); }
    if base == ".gitignore" || base == ".gitattributes" { return ("", Some(IconName::File), Some("git")); }
    if base == "package.json" || base == "package-lock.json" { return ("npm", None, Some("npm")); }
    if base == ".env" || base.starts_with(".env.") { return ("", Some(IconName::Lock), Some("env")); }
    let t = |m: &'static str, k: &'static str| (m, None, Some(k));
    match ext {
        ".js" | ".mjs" | ".cjs" => t("JS", "js"), ".ts" => t("TS", "ts"), ".tsx" => t("TX", "ts"), ".jsx" => t("JX", "jsx"),
        ".json" => t("{}", "json"), ".html" => t("◇", "html"), ".css" => t("#", "css"), ".scss" => t("#", "scss"), ".py" => t("PY", "py"),
        ".go" => t("Go", "go"), ".rs" => t("RS", "rs"), ".java" => t("JV", "java"), ".c" => t("C", "c"), ".cpp" => t("C+", "cpp"),
        ".h" => t("H", "c"), ".sh" => t("$_", "sh"), ".ps1" => t("PS", "ps1"), ".sql" => t("DB", "sql"), ".md" => t("M↓", "md"),
        ".yaml" | ".yml" => t("YML", "yaml"), ".xml" => t("XML", "xml"), ".vue" => t("V", "vue"), ".svelte" => t("S", "svelte"),
        ".svg" => ("", Some(IconName::Image), Some("svg")),
        ".png" | ".jpg" | ".jpeg" | ".gif" | ".webp" => ("", Some(IconName::Image), None),
        ".zip" | ".tar" | ".gz" => ("", Some(IconName::Archive), None),
        ".pdf" => ("", Some(IconName::FileText), Some("pdf")), ".lock" => ("", Some(IconName::Lock), Some("lock")), ".log" => ("", Some(IconName::List), None),
        _ => ("", Some(IconName::File), None),
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

/// What is wrong with a new file's name: one name in the folder, never a
/// path (the server says the same, and refuses a name that is taken).
pub fn new_file_name_error(name: &str) -> Option<&'static str> {
    if name.is_empty() { return Some("Enter a name"); }
    if name.contains(['/', '\\']) { return Some("A name cannot contain / or \\"); }
    if name == "." || name == ".." { return Some("That name is not allowed"); }
    None
}

/// Where a row's New file goes: inside a folder; beside a file (a search
/// lists files from other folders, so its own folder is taken from its path).
pub fn new_file_dir(path: &str, is_dir: bool, here: &str, searching: bool) -> String {
    if is_dir { return path.to_string(); }
    if !searching { return here.to_string(); }
    match path.rfind(['/', '\\']) { Some(0) => path[..1].to_string(), Some(i) => path[..i].to_string(), None => here.to_string() }
}

/// POST /api/files/create's answer: the new file's path, or the toast's words.
/// An older server has no such route (its catch-all 404 "Not found").
pub fn create_answer(r: Result<Value, crundi_core::api::ApiError>) -> Result<String, String> {
    use crundi_core::api::ApiError;
    match r {
        Ok(v) if v.get("ok").is_some_and(truthy) => Ok(s(&v, "path")),
        Ok(v) => Err(format!("New file failed: {}", err_of(&v, "unknown"))),
        Err(ApiError::Status(404, m)) if m == "Not found" => Err("New file is not supported by this server".into()),
        Err(ApiError::Status(_, m)) if !m.is_empty() => Err(format!("New file failed: {m}")),
        Err(e) => Err(format!("New file failed: {e}")),
    }
}

/// The row menu's items (openFileMenu). `fm`: the file manager's name when
/// the server runs here; `clip`: how many are waiting to be pasted.
pub fn row_menu_items(dir: bool, name: &str, fm: Option<&str>, clip: Option<usize>, inside: bool) -> Vec<w::MenuEntry> {
    let mut items = vec![w::MenuEntry::new("open", "Open").icon(if dir { IconName::Folder } else { IconName::File })];
    if !dir && wins::is_source_preview(name) { items.push(w::MenuEntry::new("source", "Edit the source").icon(IconName::Pencil)); }
    items.push(w::MenuEntry::new("new-file", "New file").icon(IconName::Plus).sep());
    if let Some(fm) = fm {
        if !dir { items.push(w::MenuEntry::new("ext", "Open externally").icon(IconName::ExternalLink).sep()); }
        let it = w::MenuEntry::new(if dir { "shell-open" } else { "reveal" }, if dir { format!("Open in {fm}") } else { format!("Reveal in {fm}") }).icon(IconName::Folder);
        items.push(if dir { it.sep() } else { it });
    }
    items.push(w::MenuEntry::new("cut", "Cut").icon(IconName::Scissors).sep());
    items.push(w::MenuEntry::new("copy", "Copy").icon(IconName::Copy));
    if let Some(n) = clip {
        let what = if n == 1 { String::new() } else { format!(" {n} items") };
        if dir { items.push(w::MenuEntry::new("paste-into", format!("Paste{what} inside \u{201c}{name}\u{201d}")).icon(IconName::Clipboard)); }
        if inside { items.push(w::MenuEntry::new("paste-here", format!("Paste{what}{}", if dir { " in the current folder" } else { " here" })).icon(IconName::Clipboard)); }
    }
    items.push(w::MenuEntry::new("path", "Copy path").icon(IconName::Copy).sep());
    if !dir { items.push(w::MenuEntry::new("download", "Download").icon(IconName::Download)); }
    items.push(w::MenuEntry::new("delete", "Delete").icon(IconName::Trash).danger().sep());
    items
}

/// The menu on the list's empty space: the folder on show.
pub fn space_menu_items(clip: Option<usize>) -> Vec<w::MenuEntry> {
    let mut items = vec![w::MenuEntry::new("new-file", "New file").icon(IconName::Plus)];
    if let Some(n) = clip { items.push(w::MenuEntry::new("paste-here", format!("Paste{} here", if n == 1 { String::new() } else { format!(" {n} items") })).icon(IconName::Clipboard).sep()); }
    items
}

#[derive(Clone)]
struct Clip { op: String, project: String, paths: Vec<String> }

/// One row as drawn and listed.
#[derive(Clone)]
pub struct Row { pub path: String, pub name: String, pub dir: String, pub is_dir: bool, pub up: bool, pub size: String, pub mark: String, pub icon: Option<IconName>, pub tone: Option<&'static str>, pub cut: bool, pub selected: bool }

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
    /// The rows on show, the search's note and the empty line (`derive`).
    rows: Vec<Row>,
    note: String,
    empty: String,
    /// The project's folder (a row's absolute path, dragged into a chat).
    root: String,
    project: String,
    search: Entity<w::InputState>,
    /// The window the search box is in (a load's answer empties it).
    win: AnyWindowHandle,
    list: UniformListScrollHandle,
    /// Where each drawn row's "…" is in the window, recorded as it is laid
    /// out: its menu opens below it (the web's openFileMenu at the button's
    /// right − 4, bottom + 2), and the driver's state shows it so a test
    /// clicks it as a person would.
    more_at: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, Bounds<Pixels>>>>,
    /// The list's width (a dragged row's ghost is the row at this width).
    width: Pixels,
    _sub: Subscription,
}

pub struct FilesHandle(pub Entity<FilesView>);
impl Global for FilesHandle {}
pub fn view(cx: &App) -> Option<Entity<FilesView>> { cx.try_global::<FilesHandle>().map(|h| h.0.clone()) }

impl FilesView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| w::InputState::new(window, cx).placeholder("Search files & folders…"));
        let _sub = cx.subscribe_in(&search, window, |f, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::Change = ev { let q = st.read(cx).value().to_string(); f.search(&q, cx); });
        FilesView { path: String::new(), parent: None, inside: false, entries: vec![], crumbs: vec![], shell: None, state: "loading".into(), error: String::new(), q: String::new(), results: None, search_gen: 0, selected: String::new(), clip: None, copied: String::new(),
            rows: vec![], note: String::new(), empty: String::new(), root: String::new(), project: crundi_shell::project(cx), search, win: window.window_handle(), list: UniformListScrollHandle::new(), more_at: Default::default(), width: px(320.), _sub }
    }

    fn clip_here(&self, cx: &App) -> bool { self.clip.as_ref().is_some_and(|c| c.project == crundi_shell::project(cx) && !c.paths.is_empty()) }

    fn row(&self, e: &Value, dir: Option<String>) -> Row {
        let path = s(e, "path");
        let name = s(e, "name");
        let is_dir = s(e, "type") == "dir";
        let (mark, icon, tone) = if is_dir { ("", Some(IconName::Folder), None) } else { file_mark(&name) };
        Row {
            cut: self.clip.as_ref().is_some_and(|c| c.op == "cut" && c.paths.contains(&path)), selected: self.selected == path,
            size: if is_dir { String::new() } else { fmt_size(e.get("size").and_then(|v| v.as_f64()).unwrap_or(0.)) },
            path, name, dir: dir.unwrap_or_default(), is_dir, up: false, mark: mark.into(), icon, tone,
        }
    }

    /// The rows on show again, after what they come from changed (once per
    /// change; render only lays them out).
    fn derive(&mut self, cx: &mut Context<Self>) {
        (self.rows, self.note, self.empty) = self.rows();
        self.project = crundi_shell::project(cx);
        self.root = crundi_shell::stores(cx).projects.read(cx).path_of(&self.project).trim_end_matches('/').to_string();
        cx.notify();
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
            if let Some(p) = &self.parent { rows.push(Row { path: p.clone(), name: "..".into(), dir: String::new(), is_dir: true, up: true, size: String::new(), mark: String::new(), icon: Some(IconName::ArrowUp), tone: None, cut: false, selected: false }); }
            for e in &self.entries { rows.push(self.row(e, None)); }
        }
        (rows, note, empty)
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.path.clear(); self.parent = None; self.inside = false; self.entries.clear(); self.crumbs.clear(); self.state = "loading".into(); self.q.clear(); self.results = None; self.selected.clear();
        self.derive(cx);
    }

    /// GET /api/files/list (dir: an absolute path; none: the one on show, or
    /// the project). A folder that went falls back, with a toast.
    pub fn load(&mut self, dir: Option<String>, cx: &mut Context<Self>) {
        let project = crundi_shell::project(cx);
        if project.is_empty() { return self.derive(cx); }
        let target = dir.unwrap_or_else(|| self.path.clone());
        let path = format!("/api/files/list?project={}&dir={}", crate::enc(&project), crate::enc(&target));
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
            if crundi_shell::project(cx) != project { return; }
            let mut fell = None;
            let mut cleared = false;
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
                        cleared = true;
                    }
                    Ok(v) => { f.state = "error".into(); f.error = err_of(&v, "Failed"); }
                    Err(e) => { f.state = "error".into(); f.error = format!("Error: {e}"); }
                }
                f.derive(cx);
            });
            // A load empties the search box too (the web's filesSearchQuery = '').
            if cleared { if let Some(f) = me.upgrade() {
                let (st, win) = { let f = f.read(cx); (f.search.clone(), f.win) };
                if !st.read(cx).value().is_empty() { let _ = win.update(cx, |_, window, cx| st.update(cx, |st, cx| st.set_value("", window, cx))); }
            } }
            if let Some(p) = fell { let b = base_name(&p); toast(cx, format!("That folder no longer exists. Showing {} instead.", if b.is_empty() { p } else { b }), w::Toast::Info); }
        });
    }

    pub fn up(&mut self, cx: &mut Context<Self>) { if let Some(p) = self.parent.clone() { self.load(Some(p), cx); } }

    /// Typing in the search box: the results 220ms after the last key.
    pub fn search(&mut self, q: &str, cx: &mut Context<Self>) {
        self.q = q.to_string();
        self.search_gen += 1;
        let g = self.search_gen;
        if q.trim().is_empty() { self.results = None; return self.derive(cx); }
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
                        f.derive(cx);
                    });
                });
            });
        }).detach();
    }

    /// Text typed into the search box (the test driver): the box reads it.
    pub fn type_search(&mut self, q: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |st, cx| st.set_value(q.to_string(), window, cx));
        self.search(q, cx);
    }

    /// Esc in the search: it empties, the folder shows again.
    pub fn search_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |st, cx| st.set_value("", window, cx));
        self.search("", cx);
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
        let clip = self.clip.as_ref().filter(|_| self.clip_here(cx)).map(|c| c.paths.len());
        let here = self.path.clone();
        let fm = self.shell.as_ref().map(|sh| s(sh, "fileManager"));
        let items = row_menu_items(dir, &name, fm.as_deref(), clip, self.inside);
        let new_in = new_file_dir(path, dir, &here, self.results.is_some());
        let p = path.to_string();
        let me = cx.entity().downgrade();
        w::menu_at(cx, at, items, move |id, window, cx| {
            let p = p.clone();
            let here = here.clone();
            let _ = me.update(cx, |f, cx| match id.as_str() {
                "open" => f.open(&p, cx),
                "new-file" => f.new_file(new_in.clone(), window, cx),
                "source" => wins::open_source(&crundi_shell::project(cx), &p, cx),
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
        self.derive(cx);
    }

    /// Right-click on the list but not on a row (a full folder has the bar's "+" too).
    pub fn space_menu(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        if !self.inside { return; }
        let clip = self.clip.as_ref().filter(|_| self.clip_here(cx)).map(|c| c.paths.len());
        let me = cx.entity().downgrade();
        w::menu_at(cx, at, space_menu_items(clip), move |id, window, cx| {
            let _ = me.update(cx, |f, cx| match id.as_str() {
                "new-file" => { let d = f.path.clone(); f.new_file(d, window, cx) }
                "paste-here" => f.paste_here(cx),
                _ => {}
            });
        });
    }

    /// New file: its name asked, an empty file made in `dir` (never over one
    /// that is there: /api/files/create), then opened in the editor.
    pub fn new_file(&mut self, dir: String, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.entity().downgrade();
        let a = w::Ask { title: "New file".into(), message: "File name".into(), ok: "Create".into(), prompt: Some(String::new()), ..Default::default() };
        w::ask(window, cx, a, move |v, _, cx| {
            let Some(raw) = v else { return };
            let name = raw.trim().to_string();
            if let Some(e) = new_file_name_error(&name) { return toast(cx, format!("New file failed: {e}"), w::Toast::Error); }
            let _ = me.update(cx, |f, cx| f.create(dir.clone(), name, cx));
        });
    }

    fn create(&mut self, dir: String, name: String, cx: &mut Context<Self>) {
        if dir == self.path && self.entries.iter().any(|e| s(e, "name") == name) {
            return toast(cx, format!("New file failed: \u{201c}{name}\u{201d} already exists"), w::Toast::Error);
        }
        let project = crundi_shell::project(cx);
        let Some(api) = crundi_shell::api(cx) else { return };
        let me = cx.entity().downgrade();
        let body = json!({ "project": project, "dir": dir, "name": name });
        crundi_shell::run(cx, move || api.post("/api/files/create", &body), move |r, cx| {
            match create_answer(r) {
                Ok(path) => { toast(cx, format!("Created {name}"), w::Toast::Success); wins::open_file(&project, &path, cx); }
                Err(e) => toast(cx, e, w::Toast::Error),
            }
            let _ = me.update(cx, |f, cx| f.load(None, cx));
        });
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
        self.derive(cx);
    }

    pub fn paste_here(&mut self, cx: &mut Context<Self>) { let here = self.path.clone(); self.paste(&here, cx); }

    /// Paste once into a folder: /api/files/transfer.
    fn paste(&mut self, to: &str, cx: &mut Context<Self>) {
        if !self.clip_here(cx) { return; }
        let Some(c) = self.clip.take() else { return };
        let op = if c.op == "cut" { "move" } else { "copy" };
        self.derive(cx);
        self.transfer(op, c.paths, to, cx);
    }

    /// /api/files/transfer: move or copy into a folder, then the list again.
    fn transfer(&mut self, op: &'static str, paths: Vec<String>, to: &str, cx: &mut Context<Self>) {
        let body = json!({ "project": crundi_shell::project(cx), "op": op, "items": paths, "to": to });
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
    fn abs_path(&self, p: &str) -> String {
        if p.starts_with('/') || p.contains(":\\") { return p.to_string(); }
        let root = self.root.clone();
        if root.is_empty() { p.to_string() } else if p.is_empty() { root } else { format!("{root}/{}", p.trim_start_matches('/')) }
    }

    /// A Files row dropped on a folder (a row, "..", or a folder in the path
    /// bar): which, since a drag does not say (filesDropMenu): Move to,
    /// Copy to, Cancel.
    pub fn drop_on_folder(&mut self, from: &str, to: &str, name: &str, at: Point<Pixels>, cx: &mut Context<Self>) {
        if from == to || !self.rows.iter().any(|r| r.path == from && !r.up) { return; }
        let items = vec![
            w::MenuEntry::new("move", format!("Move to \u{201c}{name}\u{201d}")).icon(IconName::Scissors),
            w::MenuEntry::new("copy", format!("Copy to \u{201c}{name}\u{201d}")).icon(IconName::Copy),
            w::MenuEntry::new("cancel", "Cancel").icon(IconName::X).sep(),
        ];
        let (from, to) = (from.to_string(), to.to_string());
        let me = cx.entity().downgrade();
        w::menu_at(cx, at, items, move |id, _, cx| {
            let op = match id.as_str() { "move" => "move", "copy" => "copy", _ => return };
            let _ = me.update(cx, |f, cx| f.transfer(op, vec![from.clone()], &to, cx));
        });
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
        let (rows, note, empty) = (&self.rows, &self.note, &self.empty);
        json!({
            "state": if crundi_shell::project(cx).is_empty() { "none".to_string() } else { self.state.clone() },
            "path": self.path, "inside": self.inside, "parent": self.parent, "searching": self.results.is_some(), "query": self.search.read(cx).value().to_string(), "selected": self.selected,
            "note": note, "empty": empty,
            "clip": self.clip.as_ref().map(|c| json!({ "op": c.op, "paths": c.paths })),
            "rows": rows.iter().map(|r| json!({ "name": r.name, "dir": r.dir, "path": r.path, "isDir": r.is_dir, "size": r.size, "mark": r.mark, "cut": r.cut, "selected": r.selected,
                "more": self.more_at.borrow().get(&r.path).map(|b| [f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height)]) })).collect::<Vec<_>>(),
            "crumbs": self.crumbs.iter().map(|c| s(c, "name")).collect::<Vec<_>>(),
        })
    }

    /// A row's mark: a folder yellow, a file's in its kind's colour.
    fn mark(r: &Row) -> AnyElement {
        let tone = if r.is_dir && !r.up { T::yellow() } else { r.tone.map(T::ws_ft).unwrap_or(T::text_muted()) };
        match (&r.icon, r.mark.is_empty()) {
            (Some(icon), true) => div().text_color(tone).child(w::icon(icon.clone(), T::ICON_MD)).into_any_element(),
            _ => div().text_size(T::TEXT_10).font_weight(T::W_BOLD).font_family(T::MONO).text_color(tone).child(r.mark.clone()).into_any_element(),
        }
    }

    /// A row's look (.file-item; the dragged ghost is the row itself).
    fn cells<E: Styled + ParentElement>(r: &Row, d: E) -> E {
        d.flex().items_center().gap(T::SP_8).h(T::CONTROL_SM).px(T::SP_12).text_size(T::TEXT_12).font_family(T::MONO)
            .child(div().flex_none().w(T::ICON_LG).flex().justify_center().child(Self::mark(r)))
            .child(div().flex().flex_1().min_w_0().items_center()
                .when(!r.dir.is_empty(), |d| d.child(div().text_color(T::text_muted()).truncate().child(r.dir.clone())))
                .child(div().text_color(T::text()).truncate().child(r.name.clone())))
            .child(div().flex_none().text_size(T::TEXT_11).text_color(T::text_muted()).child(r.size.clone()))
    }

    fn line(&self, r: &Row, i: usize, cx: &mut Context<Self>) -> AnyElement {
        let (p1, p2, p3) = (r.path.clone(), r.path.clone(), r.path.clone());
        let abs = self.abs_path(&r.path);
        let row = div().id(("file", i)).group("file-row").w_full().cursor_pointer();
        let row = if r.up { row } else {
            let r2 = r.clone();
            w::drag_ref_row(row, if r.is_dir { "folder" } else { "file" }, &abs, r.name.clone(), self.width, move || Self::cells(&r2, div().w_full().bg(T::bg_hover())).into_any_element())
        };
        // A folder (a row or "..") takes a Files row dropped on it: move or copy.
        let row = if r.is_dir {
            let (to, name) = (r.path.clone(), if r.up { "the parent folder".to_string() } else { r.name.clone() });
            row.drag_over::<w::RefDrag>(|s, _, _, _| s.border_2().border_dashed().border_color(T::accent()).bg(T::accent_dim()))
                .on_drop(cx.listener(move |f, d: &w::RefDrag, window, cx| { cx.stop_propagation(); let at = window.mouse_position(); f.drop_on_folder(&d.value, &to, &name, at, cx); }))
        } else { row };
        let tip = if r.up { "Double-click to go up" } else { "Double-click to open. Right-click for more. Drag onto a terminal to insert its path." };
        let more_on = r.selected;
        Self::cells(r, w::tip(row, tip).flex()
            .when(r.selected, |d| d.bg(T::accent_dim()).border_l_2().border_color(T::accent())).hover(|s| s.bg(T::bg_hover()))
            .when(r.cut, |d| d.opacity(T::WS_O_CUT))
            .on_click(cx.listener(move |f, e: &ClickEvent, _, cx| { if e.click_count() >= 2 { f.open(&p1, cx) } else if f.selected != p1 { f.selected = p1.clone(); f.derive(cx); } }))
            .when(!r.up, |d| d.on_mouse_down(MouseButton::Right, cx.listener(move |f, e: &MouseDownEvent, _, cx| { cx.stop_propagation(); f.menu(&p2, e.position, cx) }))))
            .when(!r.up, |d| {
                let (at, p4) = (self.more_at.clone(), r.path.clone());
                // "More" (.fi-more): shown on the row's hover, or while it is selected.
                d.child(div().relative().flex_none().when(!more_on, |d| d.opacity(0.).group_hover("file-row", |s| s.opacity(1.)))
                    .child(w::icon_button(("file-more", i), IconName::Ellipsis, "More").on_click(cx.listener(move |f, e: &ClickEvent, _, cx| {
                        // Under the button, as the web's; the click's point if it was never laid out.
                        let at = f.more_at.borrow().get(&p3).map(|b| point(b.right() - px(4.), b.bottom() + px(2.))).unwrap_or(e.position());
                        f.menu(&p3, at, cx)
                    })))
                    .child(canvas(move |b, _, _| { at.borrow_mut().insert(p4.clone(), b); }, |_, _, _, _| {}).absolute().top_0().left_0().size_full()))
            })
            .into_any_element()
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
        let owner = crundi_shell::stores(cx).projects.read(cx).paths().into_iter()
            .filter(|(_, r)| { let r = r.trim_end_matches('/'); !r.is_empty() && (p == r || p.starts_with(&format!("{r}/"))) })
            .max_by_key(|(_, r)| r.len());
        // As the web's openPathFromChat: the file by its path in the project.
        if let Some((alias, r)) = owner { let r = r.trim_end_matches('/'); let rel = p.get(r.len() + 1..).unwrap_or_default().to_string(); return wins::open_file(&alias, &rel, cx); }
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
        crundi_core::driver::rendered("files");
        let root = div().id("files-view").flex().flex_col().size_full().bg(T::bg());
        // Before the first answer (and with no project): nothing yet, as the web's.
        if self.project.is_empty() || self.state == "loading" { return root; }
        if self.state == "error" { return root.child(empty_line(self.error.clone())); }
        let (note, empty) = (self.note.clone(), self.empty.clone());
        // The path bar (.files-crumbs): each folder a link; a Files row dropped on one moves or copies there.
        let mut crumbs = div().flex().flex_1().min_w_0().items_center().gap(T::SP_4).overflow_hidden().text_size(T::TEXT_12);
        if self.crumbs.is_empty() { crumbs = crumbs.child(div().text_color(T::text_muted()).child("—")); }
        for (i, c) in self.crumbs.clone().iter().enumerate() {
            if i > 0 { crumbs = crumbs.child(div().text_color(T::text_muted()).opacity(T::WS_CRUMB_SEP_O).child("/")); }
            let (p, p2, name) = (s(c, "path"), s(c, "path"), s(c, "name"));
            let n2 = name.clone();
            crumbs = crumbs.child(w::link(("crumb", i), name)
                .drag_over::<w::RefDrag>(|s, _, _, _| s.border_2().border_dashed().border_color(T::accent()).bg(T::accent_dim()))
                .on_drop(cx.listener(move |f, d: &w::RefDrag, window, cx| { cx.stop_propagation(); let at = window.mouse_position(); f.drop_on_folder(&d.value, &p2, &n2, at, cx); }))
                .on_click(cx.listener(move |f, _, _, cx| f.load(Some(p.clone()), cx))));
        }
        let clip = self.clip.clone().filter(|c| self.inside && c.project == self.project && !c.paths.is_empty());
        let paste_tip = clip.as_ref().map(|c| format!("Paste {} into this folder ({})", if c.paths.len() == 1 { base_name(&c.paths[0]) } else { format!("{} items", c.paths.len()) }, if c.op == "cut" { "move" } else { "copy" })).unwrap_or_default();
        let bar = div().flex().flex_none().items_center().gap(T::SP_8).pl(T::SP_12).pr(T::SP_8).py(T::SP_6).child(crumbs)
            .child(div().flex().flex_none().gap(T::SP_4)
                .when(clip.is_some(), |d| d.child(w::icon_button("files-paste", IconName::Clipboard, paste_tip).on_click(cx.listener(|f, _, _, cx| f.paste_here(cx)))))
                .when(self.parent.is_some(), |d| d.child(w::icon_button("files-up", IconName::ArrowUp, "Up to parent folder").on_click(cx.listener(|f, _, _, cx| f.up(cx)))))
                .when(self.inside, |d| d.child(w::icon_button("files-new", IconName::Plus, "New file").on_click(cx.listener(|f, _, window, cx| { let d = f.path.clone(); f.new_file(d, window, cx) }))))
                .when(self.inside, |d| d.child(w::icon_button("files-upload", IconName::Upload, "Upload to this folder").on_click(cx.listener(|f, _, _, cx| f.upload(cx))))));
        // The search (#files-search), inside the project only; Esc empties it.
        let search = self.inside.then(|| div().id("files-search").flex_none().border_b_1().border_color(T::border_subtle())
            .on_key_down(cx.listener(|f, e: &KeyDownEvent, window, cx| if e.keystroke.key == "escape" { f.search_escape(window, cx); cx.stop_propagation(); }))
            .child(w::input(&self.search).appearance(false).font_family(T::MONO)));
        let mut list = div().flex().flex_col().flex_1().min_h_0()
            .on_mouse_down(MouseButton::Right, cx.listener(|f, e: &MouseDownEvent, _, cx| f.space_menu(e.position, cx)));
        if !note.is_empty() { list = list.child(div().flex_none().px(T::SP_12).py(T::SP_6).border_b_1().border_color(T::border_subtle()).text_size(T::TEXT_11).text_color(T::text_muted()).child(note)); }
        if self.rows.is_empty() { if !empty.is_empty() { list = list.child(empty_line(empty)); } }
        else {
            let me = cx.entity();
            let measure = canvas(move |b, _, cx| {
                let wd = b.size.width;
                if (me.read(cx).width - wd).abs() > px(0.5) { let me = me.clone(); cx.defer(move |cx| me.update(cx, |f, _| f.width = wd)); }
            }, |_, _, _, _| {}).absolute().top_0().left_0().size_full();
            let ul = uniform_list("files-list", self.rows.len(), cx.processor(|f, range: std::ops::Range<usize>, _, cx| {
                range.filter_map(|i| f.rows.get(i).cloned().map(|r| (i, r))).map(|(i, r)| f.line(&r, i, cx)).collect::<Vec<_>>()
            })).track_scroll(&self.list).size_full();
            // While a row is dragged near its edges, the list scrolls.
            list = list.child(w::edge_scroll::<w::RefDrag>(div().id("files-results"), w::Scroller::Uniform(self.list.clone()), false, true, cx.entity_id())
                .relative().w_full().flex_1().min_h_0().child(measure).child(ul));
        }
        root.child(div().flex_none().border_b_1().border_color(T::border_subtle()).when_some(search, |d, x| d.child(x)).child(bar))
            .child(list)
    }
}

/// The panel's lone line (.git-empty): an empty folder, no matches, an error.
fn empty_line(text: impl Into<SharedString>) -> Div {
    div().w_full().flex().justify_center().px(T::SP_12).py(T::SP_20).text_size(T::TEXT_12).text_color(T::text_muted()).child(text.into())
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
    use super::{base_name, create_answer, file_mark, fmt_size, new_file_dir, new_file_name_error, row_menu_items, search_note, space_menu_items};
    use crundi_core::api::ApiError;
    use serde_json::json;

    #[test]
    fn new_file_names() {
        assert_eq!(new_file_name_error("notes.md"), None);
        assert_eq!(new_file_name_error(".env"), None);
        assert_eq!(new_file_name_error(""), Some("Enter a name"));
        assert!(new_file_name_error("a/b").is_some());
        assert!(new_file_name_error("a\\b").is_some());
        assert!(new_file_name_error("..").is_some());
        assert!(new_file_name_error(".").is_some());
    }

    #[test]
    fn new_file_on_every_menu() {
        let has = |v: &Vec<crundi_widgets::MenuEntry>| v.iter().position(|e| e.id == "new-file" && e.label.as_ref() == "New file");
        // A file, a folder (with and without a clip and the shell), and empty space.
        for (dir, fm, clip) in [(false, None, None), (true, None, None), (false, Some("Files"), Some(2)), (true, Some("Files"), Some(1))] {
            let items = row_menu_items(dir, "x", fm, clip, true);
            let i = has(&items).expect("New file on a row");
            assert!(i <= 2 && items[i].sep, "near the top, after a separator");
        }
        assert_eq!(has(&space_menu_items(None)), Some(0));
        assert_eq!(has(&space_menu_items(Some(3))), Some(0));
    }

    #[test]
    fn new_file_folder_and_answer() {
        assert_eq!(new_file_dir("/p/sub", true, "/p", false), "/p/sub");
        assert_eq!(new_file_dir("/p/a.txt", false, "/p", false), "/p");
        assert_eq!(new_file_dir("/p/deep/a.txt", false, "/p", true), "/p/deep");
        assert_eq!(create_answer(Ok(json!({"ok": true, "path": "/p/n.md"}))), Ok("/p/n.md".into()));
        assert_eq!(create_answer(Err(ApiError::Status(409, "\u{201c}n.md\u{201d} already exists".into()))), Err("New file failed: \u{201c}n.md\u{201d} already exists".into()));
        assert_eq!(create_answer(Err(ApiError::Status(404, "Not found".into()))), Err("New file is not supported by this server".into()));
        assert_eq!(create_answer(Err(ApiError::Status(404, "Folder not found".into()))), Err("New file failed: Folder not found".into()));
    }

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
