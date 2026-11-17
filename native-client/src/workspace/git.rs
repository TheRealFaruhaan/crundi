//! Git (the web's loadGitInfo, gitPanelHtml and the git* actions): the
//! project's branch and changed files, staging, discarding, the diff (a
//! window, wins.rs), commit, push and pull. Refreshed every 8 seconds while
//! the Git tab or a Git pane is on show (the web: the tab only).

use super::{g, shown};
use crate::{kit, MainWindow, WorkspaceGitFile};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct St {
    info: Option<Value>,
    state: String,
    error: String,
    pulling: bool,
    pushing: bool,
    staged: Rc<VecModel<WorkspaceGitFile>>,
    changes: Rc<VecModel<WorkspaceGitFile>>,
    timer: Option<slint::Timer>,
    loading: bool,
}

thread_local! { static ST: RefCell<St> = RefCell::new(St { state: "loading".into(), ..Default::default() }); }

fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { ST.with(|s| f(&mut s.borrow_mut())) }

/// A changed file's row (renderGitFile): the letter for its side (staged:
/// the index's, else the work tree's; "?" untracked), its folder and name.
pub fn file_row(f: &Value) -> WorkspaceGitFile {
    let file = kit::s(f, "file");
    let staged = f.get("staged").is_some_and(kit::truthy);
    let xy: Vec<char> = kit::s(f, "xy").chars().collect();
    let pick = |c: Option<&char>| c.map(|c| if *c == '?' { '?' } else { *c }).unwrap_or(' ');
    let code = if staged { pick(xy.first()) } else { pick(xy.get(1)) };
    let (dir, name) = match file.rfind('/') { Some(i) => (file[..=i].to_string(), file[i + 1..].to_string()), None => (String::new(), file.clone()) };
    let num = |k: &str| f.get(k).and_then(|v| v.as_i64()).filter(|n| *n != 0).map(|n| n.to_string()).unwrap_or_default();
    WorkspaceGitFile { file: file.into(), dir: dir.into(), name: name.into(), code: code.to_string().trim().into(), add: num("add").into(), del: num("del").into(), staged }
}

/// ↑ahead ↓behind.
pub fn sync_line(info: &Value) -> String {
    let mut p = vec![];
    if let Some(a) = info.get("ahead").and_then(|v| v.as_i64()).filter(|n| *n > 0) { p.push(format!("↑{a}")); }
    if let Some(b) = info.get("behind").and_then(|v| v.as_i64()).filter(|n| *n > 0) { p.push(format!("↓{b}")); }
    p.join(" ")
}

pub fn init(w: &MainWindow) {
    let ws = w.global::<crate::WorkspaceUi>();
    with(|s| {
        ws.set_git_staged(ModelRc::from(s.staged.clone()));
        ws.set_git_changes(ModelRc::from(s.changes.clone()));
    });
    ws.on_git_refresh(load);
    ws.on_git_stage(|f| stage(&f));
    ws.on_git_unstage(|f| unstage(&f));
    ws.on_git_stage_all(stage_all);
    ws.on_git_unstage_all(unstage_all);
    ws.on_git_discard(|f| discard(&f));
    ws.on_git_diff(|f, c| diff(&f, c));
    ws.on_git_commit(|m| commit(&m));
    ws.on_git_push(push);
    ws.on_git_pull(pull);
}

pub fn clear() {
    with(|s| { s.info = None; s.state = "loading".into(); s.error.clear(); s.staged.set_vec(vec![]); s.changes.set_vec(vec![]); });
    show();
}

fn show() {
    let (state, error, info, pulling, pushing) = with(|s| (s.state.clone(), s.error.clone(), s.info.clone(), s.pulling, s.pushing));
    g(|ws| {
        ws.set_git_state(if kit::project().is_empty() { "none".into() } else { state.into() });
        ws.set_git_error(error.into());
        ws.set_git_branch(info.as_ref().map(|i| kit::s(i, "branch")).unwrap_or_default().into());
        ws.set_git_sync(info.as_ref().map(sync_line).unwrap_or_default().into());
        ws.set_git_pulling(pulling);
        ws.set_git_pushing(pushing);
    });
}

/// GET /api/git/info: the branch and the files; "Not a git repository"
/// when the server says no.
pub fn load() {
    let project = kit::project();
    if project.is_empty() { return show(); }
    if with(|s| std::mem::replace(&mut s.loading, true)) { return; }
    let path = format!("/api/git/info?project={}", kit::enc(&project));
    kit::call(move |api| api.get(&path), move |r| {
        with(|s| s.loading = false);
        if kit::project() != project { return; }
        match r {
            Ok(v) if v.get("ok").is_some_and(kit::truthy) => {
                let files: Vec<Value> = v.get("files").and_then(|f| f.as_array()).cloned().unwrap_or_default();
                let staged: Vec<WorkspaceGitFile> = files.iter().filter(|f| f.get("staged").is_some_and(kit::truthy)).map(file_row).collect();
                let changes: Vec<WorkspaceGitFile> = files.iter().filter(|f| !f.get("staged").is_some_and(kit::truthy)).map(file_row).collect();
                with(|s| {
                    kit::sync_rows(&s.staged, staged);
                    kit::sync_rows(&s.changes, changes);
                    s.info = Some(v);
                    s.state = "ok".into();
                });
            }
            Ok(_) => with(|s| { s.info = None; s.state = "not-repo".into(); }),
            Err(e) => with(|s| { s.state = "error".into(); s.error = e; }),
        }
        show();
    });
}

/// The 8-second refresh, while Git is on show.
pub fn timer() {
    let on = shown("git") && !kit::project().is_empty();
    with(|s| {
        if !on { s.timer = None; return; }
        if s.timer.is_some() { return; }
        let t = slint::Timer::default();
        t.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(8), || { if shown("git") { load(); } });
        s.timer = Some(t);
    });
}

fn post_then_load(path: &'static str, body: Value, fail: &'static str) {
    kit::call(move |api| api.post(path, &body), move |r| {
        match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => kit::toast_error(&{ let e = kit::s(&v, "error"); if e.is_empty() { fail.to_string() } else { e } }),
            Err(e) => kit::toast_error(&e),
            _ => {}
        }
        load();
    });
}

pub fn stage(file: &str) { post_then_load("/api/git/stage", json!({ "project": kit::project(), "files": [file] }), "Stage failed"); }
pub fn unstage(file: &str) { post_then_load("/api/git/unstage", json!({ "project": kit::project(), "files": [file] }), "Unstage failed"); }

fn files_of(staged: bool) -> Vec<String> {
    with(|s| s.info.as_ref().and_then(|i| i.get("files")).and_then(|f| f.as_array()).map(|a| a.iter().filter(|f| f.get("staged").is_some_and(kit::truthy) == staged).map(|f| kit::s(f, "file")).collect()).unwrap_or_default())
}

pub fn stage_all() {
    let files = files_of(false);
    if !files.is_empty() { post_then_load("/api/git/stage", json!({ "project": kit::project(), "files": files }), "Stage failed"); }
}

pub fn unstage_all() {
    let files = files_of(true);
    if !files.is_empty() { post_then_load("/api/git/unstage", json!({ "project": kit::project(), "files": files }), "Unstage failed"); }
}

/// Discard asks first (the web's confirm).
pub fn discard(file: &str) {
    let f = file.to_string();
    let m = format!("Discard changes to {f}? This cannot be undone.");
    kit::confirm(kit::Confirm { title: "Discard changes", message: &m, ok: "Discard", danger: true }, move |yes| {
        if yes { post_then_load("/api/git/discard", json!({ "project": kit::project(), "file": f }), "Discard failed"); }
    });
}

/// The diff of a file (staged: against HEAD, else the work tree against
/// the index), in a window.
pub fn diff(file: &str, cached: bool) {
    let path = format!("/api/git/diff?project={}&file={}&cached={}", kit::enc(&kit::project()), kit::enc(file), if cached { 1 } else { 0 });
    let f = file.to_string();
    kit::call(move |api| api.get(&path), move |r| match r {
        Ok(v) if v.get("ok").is_some_and(kit::truthy) => super::wins::open_diff(&f, &kit::s(&v, "old"), &kit::s(&v, "new")),
        _ => kit::toast_error("Failed to load diff"),
    });
}

pub fn commit(msg: &str) {
    let msg = msg.trim().to_string();
    if msg.is_empty() { return kit::toast_error("Enter a commit message"); }
    let body = json!({ "project": kit::project(), "message": msg });
    kit::call(move |api| api.post("/api/git/commit", &body), |r| match r {
        Ok(v) if v.get("ok").is_some_and(kit::truthy) => {
            g(|ws| ws.set_git_msg_gen(ws.get_git_msg_gen() + 1));
            kit::toast_ok("Committed");
            load();
        }
        Ok(v) => kit::toast_error(&format!("Commit failed: {}", { let e = kit::s(&v, "error"); if e.is_empty() { "unknown".into() } else { e } })),
        Err(e) => kit::toast_error(&format!("Commit failed: {e}")),
    });
}

fn sync_op(push: bool) {
    if with(|s| if push { s.pushing } else { s.pulling }) { return; }
    with(|s| if push { s.pushing = true } else { s.pulling = true });
    show();
    let path = if push { "/api/git/push" } else { "/api/git/pull" };
    let body = json!({ "project": kit::project() });
    kit::call(move |api| api.post(path, &body), move |r| {
        with(|s| if push { s.pushing = false } else { s.pulling = false });
        show();
        let (done, fail) = if push { ("Pushed", "Push failed") } else { ("Pulled", "Pull failed") };
        match r {
            Ok(v) if v.get("ok").is_some_and(kit::truthy) => { kit::toast_ok(done); load(); }
            Ok(v) => kit::toast_error(&format!("{fail}: {}", { let e = kit::s(&v, "error"); if e.is_empty() { "unknown".into() } else { e } })),
            Err(e) => kit::toast_error(&format!("{fail}: {e}")),
        }
    });
}

pub fn push() { sync_op(true); }
pub fn pull() { sync_op(false); }

pub fn props() -> Value {
    with(|s| json!({
        "state": s.state,
        "branch": s.info.as_ref().map(|i| kit::s(i, "branch")).unwrap_or_default(),
        "staged": s.staged.iter().map(|f| json!({ "file": f.file.as_str(), "code": f.code.as_str() })).collect::<Vec<_>>(),
        "changes": s.changes.iter().map(|f| json!({ "file": f.file.as_str(), "code": f.code.as_str() })).collect::<Vec<_>>(),
        "pulling": s.pulling, "pushing": s.pushing,
    }))
}

use slint::Model;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows() {
        let r = file_row(&json!({ "file": "src/app.js", "xy": " M", "staged": false, "add": 3, "del": 0 }));
        assert_eq!((r.dir.as_str(), r.name.as_str(), r.code.as_str(), r.add.as_str(), r.del.as_str()), ("src/", "app.js", "M", "3", ""));
        let r = file_row(&json!({ "file": "README.md", "xy": "M ", "staged": true }));
        assert_eq!((r.dir.as_str(), r.code.as_str()), ("", "M"));
        let r = file_row(&json!({ "file": "docs/n.txt", "xy": "??", "staged": false }));
        assert_eq!(r.code.as_str(), "?");
        assert_eq!(sync_line(&json!({ "ahead": 2, "behind": 1 })), "↑2 ↓1");
        assert_eq!(sync_line(&json!({ "ahead": 0 })), "");
    }
}
