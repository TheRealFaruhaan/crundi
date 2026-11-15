//! Files (the web's loadFiles, renderFilesPanel, openFileMenu and the files*
//! actions): browse by absolute paths (the server resolves them, and the
//! path bar can go above the project), search the project, open a file in
//! a window (wins.rs), and the row menu: Open, Edit the source, Open
//! externally / Reveal (on the server's own machine), Cut, Copy, Paste,
//! Copy path, Download, Delete; Upload into the folder on show.

use super::g;
use crate::{kit, MainWindow, WorkspaceCrumb, WorkspaceFileRow};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{cell::RefCell, rc::Rc};

#[derive(Default, Clone)]
struct Clip { op: String, project: String, paths: Vec<String> }

#[derive(Default)]
struct St {
    path: String,
    parent: Option<String>,
    inside: bool,
    entries: Vec<Value>,
    crumbs: Vec<Value>,
    shell: Option<Value>,
    state: String,
    error: String,
    q: String,
    results: Option<Value>,
    search_gen: u64,
    selected: String,
    clip: Option<Clip>,
    rows: Rc<VecModel<WorkspaceFileRow>>,
    crumb_rows: Rc<VecModel<WorkspaceCrumb>>,
}

thread_local! { static ST: RefCell<St> = RefCell::new(St { state: "loading".into(), ..Default::default() }); }

fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { ST.with(|s| f(&mut s.borrow_mut())) }

/// A path's last part, whichever way its slashes lean.
pub fn base_name(p: &str) -> String { p.rsplit(['/', '\\']).next().unwrap_or("").to_string() }

/// A file's mark in the list (fileIcon): (short text or "", icon, colour key).
pub fn file_mark(name: &str) -> (&'static str, &'static str, &'static str) {
    let base = name.to_ascii_lowercase();
    let ext = base.rfind('.').map(|i| &base[i..]).unwrap_or("");
    if base == "dockerfile" || base.starts_with("dockerfile.") { return ("", "docker", "docker"); }
    if base == ".gitignore" || base == ".gitattributes" { return ("", "file", "git"); }
    if base == "package.json" || base == "package-lock.json" { return ("npm", "", "npm"); }
    if base == ".env" || base.starts_with(".env.") { return ("", "lock", "lock"); }
    match ext {
        ".js" | ".mjs" | ".cjs" => ("JS", "", "js"),
        ".ts" => ("TS", "", "ts"),
        ".tsx" => ("TX", "", "ts"),
        ".jsx" => ("JX", "", "jsx"),
        ".json" => ("{}", "", "json"),
        ".html" => ("◇", "", "html"),
        ".css" => ("#", "", "css"),
        ".scss" => ("#", "", "scss"),
        ".py" => ("PY", "", "py"),
        ".go" => ("Go", "", "go"),
        ".rs" => ("RS", "", "rs"),
        ".java" => ("JV", "", "java"),
        ".c" => ("C", "", "c"),
        ".cpp" => ("C+", "", "cpp"),
        ".h" => ("H", "", "c"),
        ".sh" => ("$_", "", "sh"),
        ".ps1" => ("PS", "", "ps"),
        ".sql" => ("DB", "", "sql"),
        ".md" => ("M↓", "", "md"),
        ".yaml" | ".yml" => ("YML", "", "yml"),
        ".xml" => ("XML", "", "xml"),
        ".svg" => ("", "image", "svg"),
        ".png" | ".jpg" | ".jpeg" | ".gif" | ".webp" => ("", "image", ""),
        ".zip" | ".tar" | ".gz" => ("", "archive", ""),
        ".pdf" => ("", "file-text", "pdf"),
        ".lock" => ("", "lock", "lock"),
        ".log" => ("", "list", ""),
        ".vue" => ("V", "", "vue"),
        ".svelte" => ("S", "", "svelte"),
        _ => ("", "file", ""),
    }
}

/// The size column (formatFileSize): "" for none.
pub fn fmt_size(bytes: f64) -> String {
    if bytes <= 0.0 { return String::new(); }
    if bytes < 1024.0 { return format!("{} B", bytes as u64); }
    if bytes < 1024.0 * 1024.0 { return format!("{:.1} KB", bytes / 1024.0); }
    format!("{:.1} MB", bytes / (1024.0 * 1024.0))
}

/// The search's note (searchResultsHtml): "3 matches · gitignored files
/// excluded", or the empty line.
pub fn search_note(n: usize, truncated: bool, gitignore: bool) -> (String, String) {
    if n == 0 { return (String::new(), format!("No matches{}", if gitignore { " (gitignored files are excluded)" } else { "" })); }
    (format!("{}{} match{}{}", n, if truncated { "+" } else { "" }, if n == 1 { "" } else { "es" }, if gitignore { " · gitignored files excluded" } else { "" }), String::new())
}

pub fn init(w: &MainWindow) {
    let ws = w.global::<crate::WorkspaceUi>();
    with(|s| { ws.set_files_rows(ModelRc::from(s.rows.clone())); ws.set_files_crumbs(ModelRc::from(s.crumb_rows.clone())); });
    ws.on_files_nav(|p| load(Some(p.to_string())));
    ws.on_files_up(up);
    ws.on_files_search(|q| search(&q));
    ws.on_files_select(|p| select(&p));
    ws.on_files_open(|p| open(&p));
    ws.on_files_menu(|p, x, y| menu(&p, x, y));
    ws.on_files_upload(upload);
    ws.on_files_paste(paste_here);
}

pub fn clear() {
    with(|s| { s.path.clear(); s.parent = None; s.inside = false; s.entries.clear(); s.crumbs.clear(); s.state = "loading".into(); s.q.clear(); s.results = None; s.selected.clear(); });
    show();
}

fn clip_here(s: &St) -> bool { s.clip.as_ref().is_some_and(|c| c.project == kit::project() && !c.paths.is_empty()) }

fn row(e: &Value, name_dir: Option<String>, s: &St) -> WorkspaceFileRow {
    let path = kit::s(e, "path");
    let name = kit::s(e, "name");
    let is_dir = kit::s(e, "type") == "dir";
    let (mark, icon, tone) = if is_dir { ("", "folder", "dir") } else { file_mark(&name) };
    WorkspaceFileRow {
        cut: s.clip.as_ref().is_some_and(|c| c.op == "cut" && c.paths.contains(&path)),
        selected: s.selected == path,
        size: if is_dir { String::new() } else { fmt_size(kit::n(e, "size")) }.into(),
        path: path.into(), name: name.into(), dir: name_dir.unwrap_or_default().into(), is_dir, up: false,
        mark: mark.into(), icon: icon.into(), tone: tone.into(),
    }
}

fn show() {
    let (rows, crumbs, st, err, inside, searching, note, empty, can_up, paste, tip) = with(|s| {
        let mut rows = vec![];
        let (mut note, mut empty) = (String::new(), String::new());
        let searching = s.results.is_some();
        if let Some(r) = &s.results {
            if let Some(e) = r.get("error").and_then(|e| e.as_str()) { empty = format!("Search error: {e}"); }
            else {
                let list = r.get("entries").and_then(|e| e.as_array()).cloned().unwrap_or_default();
                (note, empty) = search_note(list.len(), r.get("truncated").is_some_and(kit::truthy), r.get("gitignoreApplied").is_some_and(kit::truthy));
                for e in &list {
                    let rel = kit::s(e, "rel");
                    let dir = rel.rfind('/').map(|i| rel[..=i].to_string());
                    rows.push(row(e, dir, s));
                }
            }
        } else {
            if let Some(p) = &s.parent {
                rows.push(WorkspaceFileRow { path: p.clone().into(), name: "..".into(), is_dir: true, up: true, icon: "arrow-up".into(), tone: "dir".into(), ..Default::default() });
            }
            for e in &s.entries { rows.push(row(e, None, s)); }
        }
        let crumbs: Vec<WorkspaceCrumb> = s.crumbs.iter().map(|c| WorkspaceCrumb { name: kit::s(c, "name").into(), path: kit::s(c, "path").into() }).collect();
        let tip = s.clip.as_ref().map(|c| format!("Paste {} into this folder ({})", if c.paths.len() == 1 { base_name(&c.paths[0]) } else { format!("{} items", c.paths.len()) }, if c.op == "cut" { "move" } else { "copy" })).unwrap_or_default();
        (rows, crumbs, s.state.clone(), s.error.clone(), s.inside, searching, note, empty, s.parent.is_some(), s.inside && clip_here(s), tip)
    });
    with(|s| { kit::sync_rows(&s.rows, rows); kit::sync_rows(&s.crumb_rows, crumbs); });
    g(|ws| {
        ws.set_files_state(if kit::project().is_empty() { "none".into() } else { st.into() });
        ws.set_files_error(err.into());
        ws.set_files_inside(inside);
        ws.set_files_searching(searching);
        ws.set_files_note(note.into());
        ws.set_files_empty(empty.into());
        ws.set_files_can_up(can_up);
        ws.set_files_can_paste(paste);
        ws.set_files_paste_tip(tip.into());
    });
}

/// GET /api/files/list (dir: an absolute path; none: the one on show, or
/// the project). A folder that went falls back, with a toast.
pub fn load(dir: Option<String>) {
    let project = kit::project();
    if project.is_empty() { return show(); }
    let target = dir.unwrap_or_else(|| with(|s| s.path.clone()));
    let path = format!("/api/files/list?project={}&dir={}", kit::enc(&project), kit::enc(&target));
    kit::call(move |api| api.get(&path), move |r| {
        if kit::project() != project { return; }
        match r {
            Ok(v) if v.get("ok").is_some_and(kit::truthy) => {
                let fell = v.get("fellBack").is_some_and(kit::truthy);
                let p = kit::s(&v, "path");
                with(|s| {
                    s.path = p.clone();
                    s.parent = v.get("parent").and_then(|x| x.as_str()).filter(|x| !x.is_empty()).map(str::to_string);
                    s.inside = v.get("inside").is_some_and(kit::truthy);
                    s.entries = v.get("entries").and_then(|e| e.as_array()).cloned().unwrap_or_default();
                    s.crumbs = v.get("crumbs").and_then(|e| e.as_array()).cloned().unwrap_or_default();
                    s.shell = v.get("localShell").filter(|x| x.is_object()).cloned();
                    s.state = "ok".into();
                    s.q.clear();
                    s.results = None;
                });
                g(|ws| { ws.set_files_q("".into()); ws.set_files_q_gen(ws.get_files_q_gen() + 1); });
                if fell { kit::toast(&format!("That folder no longer exists. Showing {} instead.", { let b = base_name(&p); if b.is_empty() { p.clone() } else { b } })); }
            }
            Ok(v) => with(|s| { s.state = "error".into(); s.error = { let e = kit::s(&v, "error"); if e.is_empty() { "Failed".into() } else { e } }; }),
            Err(e) => with(|s| { s.state = "error".into(); s.error = format!("Error: {e}"); }),
        }
        show();
    });
}

pub fn up() { if let Some(p) = with(|s| s.parent.clone()) { load(Some(p)); } }

/// Typing in the search box: the results 220ms after the last key; Esc
/// clears (the box says "" to here).
pub fn search(q: &str) {
    let gen = with(|s| { s.q = q.to_string(); s.search_gen += 1; s.search_gen });
    if q.trim().is_empty() { with(|s| s.results = None); return show(); }
    slint::Timer::single_shot(std::time::Duration::from_millis(220), move || {
        if with(|s| s.search_gen) != gen { return; }
        let q = with(|s| s.q.trim().to_string());
        let path = format!("/api/files/search?project={}&q={}", kit::enc(&kit::project()), kit::enc(&q));
        kit::call(move |api| api.get(&path), move |r| {
            // A stale answer (more typed, or cleared) is dropped.
            if with(|s| s.q.trim().to_string()) != q { return; }
            let res = match r {
                Ok(v) if v.get("ok").is_some_and(kit::truthy) => v,
                Ok(v) => json!({ "entries": [], "error": kit::s(&v, "error") }),
                Err(e) => json!({ "entries": [], "error": e }),
            };
            with(|s| s.results = Some(res));
            show();
        });
    });
}

pub fn select(path: &str) { with(|s| s.selected = path.to_string()); show(); }

fn is_dir(path: &str) -> bool {
    with(|s| {
        if s.parent.as_deref() == Some(path) { return true; }
        let list = s.results.as_ref().and_then(|r| r.get("entries")).and_then(|e| e.as_array()).cloned().unwrap_or_default();
        s.entries.iter().chain(list.iter()).any(|e| kit::s(e, "path") == path && kit::s(e, "type") == "dir")
    })
}

/// Double click: a folder opens here, a file in a window.
pub fn open(path: &str) {
    if is_dir(path) { return load(Some(path.to_string())); }
    super::wins::open_file(&kit::project(), path, false);
}

/// The row menu (openFileMenu).
pub fn menu(path: &str, x: f32, y: f32) {
    if with(|s| s.parent.as_deref() == Some(path) && s.results.is_none()) { return; }
    select(path);
    let dir = is_dir(path);
    let name = base_name(path);
    let (shell, clip, inside, here) = with(|s| (s.shell.clone(), s.clip.clone().filter(|_| clip_here(s)), s.inside, s.path.clone()));
    let ic = |n: &str| kit::icon(move |i| match n { "folder" => i.get_folder(), "file" => i.get_file(), "pencil" => i.get_pencil(), "external-link" => i.get_external_link(), "scissors" => i.get_scissors(), "copy" => i.get_copy(), "clipboard" => i.get_clipboard(), "download" => i.get_download(), _ => i.get_trash() });
    let mut items = vec![kit::item("open", "Open").icon(ic(if dir { "folder" } else { "file" }))];
    if !dir && name.to_ascii_lowercase().ends_with(".svg") { items.push(kit::item("source", "Edit the source").icon(ic("pencil"))); }
    let mut sep = true;
    if let Some(sh) = &shell {
        let fm = kit::s(sh, "fileManager");
        if !dir { items.push(kit::item("ext", "Open externally").icon(ic("external-link")).sep()); sep = false; }
        let it = kit::item(if dir { "shell-open" } else { "reveal" }, &if dir { format!("Open in {fm}") } else { format!("Reveal in {fm}") }).icon(ic("folder"));
        items.push(if sep { it.sep() } else { it });
    }
    items.push(kit::item("cut", "Cut").icon(ic("scissors")).sep());
    items.push(kit::item("copy", "Copy").icon(ic("copy")));
    if let Some(c) = &clip {
        let what = if c.paths.len() == 1 { String::new() } else { format!(" {} items", c.paths.len()) };
        if dir { items.push(kit::item("paste-into", &format!("Paste{what} inside \u{201c}{name}\u{201d}")).icon(ic("clipboard"))); }
        if inside { items.push(kit::item("paste-here", &format!("Paste{what}{}", if dir { " in the current folder" } else { " here" })).icon(ic("clipboard"))); }
    }
    items.push(kit::item("path", "Copy path").icon(ic("copy")).sep());
    if !dir { items.push(kit::item("download", "Download").icon(ic("download"))); }
    items.push(kit::item("delete", "Delete").icon(ic("trash")).danger().sep());
    let p = path.to_string();
    kit::menu(x, y, items, move |id| match id.as_str() {
        "open" => open(&p),
        "source" => super::wins::open_file(&kit::project(), &p, true),
        "ext" => shell_act("open", &p),
        "shell-open" => shell_act("open", &p),
        "reveal" => shell_act("reveal", &p),
        "cut" => set_clip("cut", &p),
        "copy" => set_clip("copy", &p),
        "paste-into" => paste(&p),
        "paste-here" => paste(&here),
        "path" => copy_path(&p),
        "download" => download(&p),
        "delete" => delete(&p),
        _ => {}
    });
}

fn shell_act(act: &str, p: &str) {
    let body = json!({ "project": kit::project(), "file": p, "act": act });
    kit::call(move |api| api.post("/api/files/shell", &body), |r| match r {
        Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => kit::toast_error(&{ let e = kit::s(&v, "error"); if e.is_empty() { "Could not do that".into() } else { e } }),
        Err(e) => kit::toast_error(&e),
        _ => {}
    });
}

fn set_clip(op: &str, p: &str) {
    with(|s| s.clip = Some(Clip { op: op.into(), project: kit::project(), paths: vec![p.to_string()] }));
    kit::toast(&format!("{}{} Paste it into a folder.", base_name(p), if op == "cut" { " is ready to move." } else { " is ready to copy." }));
    show();
}

pub fn paste_here() { let here = with(|s| s.path.clone()); paste(&here); }

/// Paste once (whatever comes of it) into a folder: /api/files/transfer.
fn paste(to: &str) {
    let Some(c) = with(|s| if clip_here(s) { s.clip.take() } else { None }) else { return };
    let op = if c.op == "cut" { "move" } else { "copy" };
    let body = json!({ "project": kit::project(), "op": op, "items": c.paths, "to": to });
    show();
    kit::call(move |api| api.post("/api/files/transfer", &body), move |r| {
        match r {
            Ok(d) => {
                let did = d.get("done").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                let bad = d.get("failed").and_then(|x| x.as_array()).cloned().unwrap_or_default();
                if let Some(first) = did.first() {
                    let what = if did.len() == 1 { kit::s(first, "name") } else { format!("{} items", did.len()) };
                    let renamed = did.len() == 1 && first.get("renamed").is_some_and(kit::truthy);
                    kit::toast_ok(&format!("{}{}{}", if op == "move" { "Moved " } else { "Copied " }, what, if renamed { " (renamed: that name was taken)" } else { "" }));
                }
                if !bad.is_empty() || d.get("ok").and_then(|o| o.as_bool()) == Some(false) {
                    let msg = bad.first().map(|b| format!("{}: {}", base_name(&kit::s(b, "from")), kit::s(b, "error"))).unwrap_or_else(|| { let e = kit::s(&d, "error"); if e.is_empty() { format!("Could not {op}") } else { e } });
                    kit::toast_error(&msg);
                }
            }
            Err(e) => kit::toast_error(&e),
        }
        load(None);
    });
}

fn copy_path(p: &str) {
    let ok = arboard::Clipboard::new().and_then(|mut c| c.set_text(p.to_string())).is_ok();
    super::note_copy(p);
    if ok { kit::toast("Path copied"); } else { kit::toast_error("Copy failed"); }
}

/// Download: where to save first, then the file streams (the download rows).
pub fn download(p: &str) {
    let name = { let b = base_name(p); if b.is_empty() { "download".to_string() } else { b } };
    let url = format!("/api/files/download?project={}&file={}", kit::enc(&kit::project()), kit::enc(p));
    crate::app::download_url(url, name);
}

/// Delete asks first; the folder on show reloads.
fn delete(p: &str) {
    let name = base_name(p);
    let m = format!("Delete \"{name}\"?\n\nThis permanently removes it from disk and cannot be undone.");
    let p = p.to_string();
    kit::confirm(kit::Confirm { title: "Delete", message: &m, ok: "Delete", danger: true }, move |yes| {
        if !yes { return; }
        let body = json!({ "project": kit::project(), "file": p });
        kit::call(move |api| api.post("/api/files/delete", &body), move |r| {
            match r {
                Ok(v) if v.get("ok").is_some_and(kit::truthy) => kit::toast_ok(&format!("Deleted {name}")),
                Ok(v) => kit::toast_error(&format!("Delete failed: {}", { let e = kit::s(&v, "error"); if e.is_empty() { "unknown".into() } else { e } })),
                Err(e) => kit::toast_error(&format!("Delete failed: {e}")),
            }
            load(None);
        });
    });
}

/// Upload: the system's picker (several files), each into the folder on
/// show (/api/files/upload, base64), a toast each, then the list again.
pub fn upload() {
    let (project, dir) = (kit::project(), with(|s| s.path.clone()));
    if project.is_empty() { return; }
    kit::bg(|| crate::testctl::take_pick().map(|p| vec![p]).or_else(|| rfd::FileDialog::new().pick_files()).unwrap_or_default(), move |paths| {
        for p in paths {
            let (project, dir) = (project.clone(), dir.clone());
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
            let n2 = name.clone();
            kit::call(move |api| {
                use base64::Engine;
                let bytes = std::fs::read(&p).map_err(|e| crate::api::ApiError::Net(e.to_string()))?;
                api.post("/api/files/upload", &json!({ "project": project, "dir": dir, "name": n2, "data": base64::engine::general_purpose::STANDARD.encode(bytes) }))
            }, move |r| {
                match r {
                    Ok(v) if v.get("ok").is_some_and(kit::truthy) => kit::toast_ok(&format!("Uploaded {name}")),
                    Ok(v) => kit::toast_error(&format!("Upload failed: {}", { let e = kit::s(&v, "error"); if e.is_empty() { "unknown".into() } else { e } })),
                    Err(e) => kit::toast_error(&format!("Error: {e}")),
                }
                load(None);
            });
        }
    });
}

/// The web's openPathFromChat: a path inside a project opens in its editor;
/// anything else is resolved by the server first.
pub fn open_path_from_chat(project: &str, raw: &str) {
    let mut p = raw.trim().trim_start_matches("./").to_string();
    // A trailing :line or :line:col.
    for _ in 0..2 { if let Some(i) = p.rfind(':') { if i > 0 && p[i + 1..].chars().all(|c| c.is_ascii_digit()) && i + 1 < p.len() { p.truncate(i); } } }
    if p.is_empty() { return; }
    if p.starts_with('/') {
        let owner = kit::projects().into_iter().filter(|x| { let root = kit::s(x, "path"); let r = root.trim_end_matches('/'); !r.is_empty() && (p == r || p.starts_with(&format!("{r}/"))) })
            .max_by_key(|x| kit::s(x, "path").len());
        if let Some(o) = owner { return super::wins::open_file(&kit::s(&o, "alias"), &p, false); }
    }
    let proj = if project.is_empty() { kit::project() } else { project.to_string() };
    let path = format!("/api/files/resolve?project={}&path={}", kit::enc(&proj), kit::enc(&p));
    kit::call(move |api| api.get(&path), move |r| match r {
        Ok(d) if d.get("ok").is_some_and(kit::truthy) => {
            if d.get("dir").is_some_and(kit::truthy) { return kit::toast(&format!("That is a folder: {p}")); }
            super::wins::open_file(&proj, &kit::s(&d, "file"), false);
        }
        Ok(_) => kit::toast_error(&format!("File not found: {p}")),
        Err(_) => super::wins::open_file(&proj, &p, false),
    });
}

pub fn props() -> Value {
    with(|s| json!({
        "state": s.state, "path": s.path, "inside": s.inside, "parent": s.parent,
        "searching": s.results.is_some(), "selected": s.selected,
        "clip": s.clip.as_ref().map(|c| json!({ "op": c.op, "paths": c.paths })),
        "rows": s.rows.iter().map(|r| json!({ "name": r.name.as_str(), "dir": r.dir.as_str(), "path": r.path.as_str(), "isDir": r.is_dir, "size": r.size.as_str(), "mark": r.mark.as_str(), "cut": r.cut, "selected": r.selected })).collect::<Vec<_>>(),
        "crumbs": s.crumbs.iter().map(|c| kit::s(c, "name")).collect::<Vec<_>>(),
    }))
}

use slint::Model;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_and_sizes() {
        assert_eq!(file_mark("app.js"), ("JS", "", "js"));
        assert_eq!(file_mark("Dockerfile"), ("", "docker", "docker"));
        assert_eq!(file_mark("README.md"), ("M↓", "", "md"));
        assert_eq!(file_mark("x.unknown"), ("", "file", ""));
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
