//! Git (3.3; the web's loadGitInfo and gitPanelHtml, native-gpui's git.rs):
//! the project's branch with ↑ahead ↓behind; Staged and Changes with their
//! counts, Unstage all / Stage all; each file's status letter, folder, name,
//! +/− counts, View diff, Stage / Unstage, Discard (asks first); the commit
//! box with ↓ Pull, ↑ Push and Commit. Refreshed every 8 s while the tab is
//! on show.
//!
//! The server's answer becomes rows once, when it arrives (`GitView::lines`);
//! a refresh that changed nothing notifies nothing (an idle Git tab draws no
//! frames); the list draws only the rows on screen (`uniform_list`: every
//! row is a control's height).

use crate::diffview;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::time::Duration;

/// A changed file's row (renderGitFile).
#[derive(Clone, Debug, PartialEq)]
pub struct FileRow { pub file: String, pub dir: String, pub name: String, pub code: String, pub add: String, pub del: String, pub staged: bool }

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }
fn truthy(v: &Value) -> bool { v.as_bool().unwrap_or(!v.is_null()) }

/// The letter for its side (staged: the index's, else the work tree's; "?"
/// untracked), its folder and name, its counts.
pub fn file_row(f: &Value) -> FileRow {
    let file = s(f, "file");
    let staged = f.get("staged").is_some_and(truthy);
    let xy: Vec<char> = s(f, "xy").chars().collect();
    let code = if staged { xy.first() } else { xy.get(1) }.copied().unwrap_or(' ');
    let (dir, name) = match file.rfind('/') { Some(i) => (file[..=i].to_string(), file[i + 1..].to_string()), None => (String::new(), file.clone()) };
    let num = |k: &str| f.get(k).and_then(|v| v.as_i64()).filter(|n| *n != 0).map(|n| n.to_string()).unwrap_or_default();
    FileRow { file: file.clone(), dir, name, code: code.to_string().trim().to_string(), add: num("add"), del: num("del"), staged }
}

/// ↑ahead ↓behind.
pub fn sync_line(info: &Value) -> String {
    let mut p = vec![];
    if let Some(a) = info.get("ahead").and_then(|v| v.as_i64()).filter(|n| *n > 0) { p.push(format!("↑{a}")); }
    if let Some(b) = info.get("behind").and_then(|v| v.as_i64()).filter(|n| *n > 0) { p.push(format!("↓{b}")); }
    p.join(" ")
}

/// A row of the list: a section's head, or a file in it.
#[derive(Clone, Debug, PartialEq)]
pub enum Line { Section { staged: bool, n: usize }, File(FileRow, usize) }

/// The list's rows: STAGED and its files, then CHANGES and theirs.
pub fn lines(staged: &[FileRow], changes: &[FileRow]) -> Vec<Line> {
    let mut out = vec![];
    if !staged.is_empty() { out.push(Line::Section { staged: true, n: staged.len() }); out.extend(staged.iter().cloned().enumerate().map(|(i, f)| Line::File(f, i))); }
    if !changes.is_empty() { out.push(Line::Section { staged: false, n: changes.len() }); out.extend(changes.iter().cloned().enumerate().map(|(i, f)| Line::File(f, i))); }
    out
}

pub struct GitView {
    pub state: String,
    pub error: String,
    pub branch: String,
    pub sync: String,
    pub staged: Vec<FileRow>,
    pub changes: Vec<FileRow>,
    rows: Vec<Line>,
    pub pulling: bool,
    pub pushing: bool,
    loading: bool,
    project: String,
    msg: Entity<w::InputState>,
    list: UniformListScrollHandle,
}

pub struct GitHandle(pub Entity<GitView>);
impl Global for GitHandle {}

pub fn view(cx: &App) -> Option<Entity<GitView>> { cx.try_global::<GitHandle>().map(|h| h.0.clone()) }

impl GitView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let msg = cx.new(|cx| w::InputState::new(window, cx).placeholder("Commit message…"));
        GitView { state: "loading".into(), error: String::new(), branch: String::new(), sync: String::new(), staged: vec![], changes: vec![], rows: vec![], pulling: false, pushing: false, loading: false, project: crundi_shell::project(cx), msg, list: UniformListScrollHandle::new() }
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.branch.clear(); self.sync.clear(); self.state = "loading".into(); self.error.clear(); self.staged.clear(); self.changes.clear(); self.rows.clear();
        self.project = crundi_shell::project(cx);
        cx.notify();
    }

    /// GET /api/git/info: the branch and the files; "Not a git repository"
    /// when the server says no. Notifies only if what it shows changed.
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let project = crundi_shell::project(cx);
        if project != self.project { self.project = project.clone(); cx.notify(); }
        if project.is_empty() || self.loading { return; }
        self.loading = true;
        let path = format!("/api/git/info?project={}", crate::enc(&project));
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
            let _ = me.update(cx, |g, cx| {
                g.loading = false;
                if crundi_shell::project(cx) != project { return; }
                let (state, error, branch, sync, staged, changes) = match r {
                    Ok(v) if v.get("ok").is_some_and(truthy) => {
                        let files: Vec<Value> = v.get("files").and_then(|f| f.as_array()).cloned().unwrap_or_default();
                        let staged: Vec<FileRow> = files.iter().filter(|f| f.get("staged").is_some_and(truthy)).map(file_row).collect();
                        let changes: Vec<FileRow> = files.iter().filter(|f| !f.get("staged").is_some_and(truthy)).map(file_row).collect();
                        ("ok".to_string(), String::new(), s(&v, "branch"), sync_line(&v), staged, changes)
                    }
                    Ok(_) => ("not-repo".into(), String::new(), String::new(), String::new(), vec![], vec![]),
                    Err(e) => ("error".into(), e, g.branch.clone(), g.sync.clone(), g.staged.clone(), g.changes.clone()),
                };
                if (&state, &error, &branch, &sync, &staged, &changes) == (&g.state, &g.error, &g.branch, &g.sync, &g.staged, &g.changes) { return; }
                g.rows = lines(&staged, &changes);
                (g.state, g.error, g.branch, g.sync, g.staged, g.changes) = (state, error, branch, sync, staged, changes);
                cx.notify();
            });
        });
    }

    fn post_then_load(&mut self, path: &'static str, body: Value, fail: &'static str, cx: &mut Context<Self>) {
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.post(path, &body), move |r, cx| {
            match r {
                Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => { let e = s(&v, "error"); crate::toast(cx, if e.is_empty() { fail.into() } else { e }, w::Toast::Error) }
                Err(e) => crate::toast(cx, e, w::Toast::Error),
                _ => {}
            }
            let _ = me.update(cx, |g, cx| g.load(cx));
        });
    }

    pub fn stage(&mut self, file: &str, cx: &mut Context<Self>) {
        let p = crundi_shell::project(cx);
        self.post_then_load("/api/git/stage", json!({ "project": p, "files": [file] }), "Stage failed", cx);
    }
    pub fn unstage(&mut self, file: &str, cx: &mut Context<Self>) {
        let p = crundi_shell::project(cx);
        self.post_then_load("/api/git/unstage", json!({ "project": p, "files": [file] }), "Unstage failed", cx);
    }
    pub fn stage_all(&mut self, cx: &mut Context<Self>) {
        let files: Vec<String> = self.changes.iter().map(|f| f.file.clone()).collect();
        if files.is_empty() { return; }
        let p = crundi_shell::project(cx);
        self.post_then_load("/api/git/stage", json!({ "project": p, "files": files }), "Stage failed", cx);
    }
    pub fn unstage_all(&mut self, cx: &mut Context<Self>) {
        let files: Vec<String> = self.staged.iter().map(|f| f.file.clone()).collect();
        if files.is_empty() { return; }
        let p = crundi_shell::project(cx);
        self.post_then_load("/api/git/unstage", json!({ "project": p, "files": files }), "Unstage failed", cx);
    }

    /// Discard asks first (the web's confirm).
    pub fn discard(&mut self, file: &str, window: &mut Window, cx: &mut Context<Self>) {
        let f = file.to_string();
        let me = cx.entity().downgrade();
        let c = w::Confirm { title: "Discard changes".into(), message: format!("Discard changes to {f}? This cannot be undone.").into(), ok: "Discard".into(), danger: true };
        w::confirm(window, cx, c, move |_, cx| {
            let p = crundi_shell::project(cx);
            let _ = me.update(cx, |g, cx| g.post_then_load("/api/git/discard", json!({ "project": p, "file": f }), "Discard failed", cx));
        });
    }

    /// The diff of a file (staged: against HEAD, else the work tree against
    /// the index), in the diff window.
    pub fn diff(&mut self, file: &str, cached: bool, cx: &mut Context<Self>) {
        let project = crundi_shell::project(cx);
        let path = format!("/api/git/diff?project={}&file={}&cached={}", crate::enc(&project), crate::enc(file), if cached { 1 } else { 0 });
        let f = file.to_string();
        crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| match r {
            Ok(v) if v.get("ok").is_some_and(truthy) => diffview::open(&project, &f, s(&v, "old"), s(&v, "new"), cx),
            _ => crate::toast(cx, "Failed to load diff".into(), w::Toast::Error),
        });
    }

    pub fn commit(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let msg = text.trim().to_string();
        if msg.is_empty() { return w::toast(window, cx, "Enter a commit message", w::Toast::Error); }
        let body = json!({ "project": crundi_shell::project(cx), "message": msg });
        let me = cx.entity().downgrade();
        let input = self.msg.clone();
        crundi_shell::call(cx, move |api| api.post("/api/git/commit", &body), move |r, cx| {
            match r {
                Ok(v) if v.get("ok").is_some_and(truthy) => {
                    crate::with_window(cx, move |window, cx| input.update(cx, |st, cx| st.set_value("", window, cx)));
                    crate::toast(cx, "Committed".into(), w::Toast::Success);
                    let _ = me.update(cx, |g, cx| g.load(cx));
                }
                Ok(v) => { let e = s(&v, "error"); crate::toast(cx, format!("Commit failed: {}", if e.is_empty() { "unknown".into() } else { e }), w::Toast::Error) }
                Err(e) => crate::toast(cx, format!("Commit failed: {e}"), w::Toast::Error),
            }
        });
    }

    fn sync_op(&mut self, push: bool, cx: &mut Context<Self>) {
        if if push { self.pushing } else { self.pulling } { return; }
        if push { self.pushing = true } else { self.pulling = true }
        cx.notify();
        let path = if push { "/api/git/push" } else { "/api/git/pull" };
        let body = json!({ "project": crundi_shell::project(cx) });
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.post(path, &body), move |r, cx| {
            let (done, fail) = if push { ("Pushed", "Push failed") } else { ("Pulled", "Pull failed") };
            let ok = matches!(&r, Ok(v) if v.get("ok").is_some_and(truthy));
            match r {
                Ok(_) if ok => crate::toast(cx, done.into(), w::Toast::Success),
                Ok(v) => { let e = s(&v, "error"); crate::toast(cx, format!("{fail}: {}", if e.is_empty() { "unknown".into() } else { e }), w::Toast::Error) }
                Err(e) => crate::toast(cx, format!("{fail}: {e}"), w::Toast::Error),
            }
            let _ = me.update(cx, |g, cx| {
                if push { g.pushing = false } else { g.pulling = false }
                if ok { g.load(cx); }
                cx.notify();
            });
        });
    }
    pub fn push(&mut self, cx: &mut Context<Self>) { self.sync_op(true, cx) }
    pub fn pull(&mut self, cx: &mut Context<Self>) { self.sync_op(false, cx) }

    pub fn props(&self, cx: &App) -> Value {
        let state = if crundi_shell::project(cx).is_empty() { "none".to_string() } else { self.state.clone() };
        json!({
            "state": state, "branch": self.branch, "sync": self.sync,
            "staged": self.staged.iter().map(|f| json!({ "file": f.file, "code": f.code })).collect::<Vec<_>>(),
            "changes": self.changes.iter().map(|f| json!({ "file": f.file, "code": f.code })).collect::<Vec<_>>(),
            "pulling": self.pulling, "pushing": self.pushing, "rows": self.rows.len(),
        })
    }

    // ─── Drawing ───

    fn section(staged: bool, n: usize, cx: &mut Context<Self>) -> AnyElement {
        let (title, act, tip) = if staged { ("STAGED", IconName::Minus, "Unstage all") } else { ("CHANGES", IconName::Plus, "Stage all") };
        div().w_full().flex().items_center().gap(T::SP_6).h(T::CONTROL_SM).px(T::SP_10).bg(T::bg_secondary())
            .child(div().text_size(T::TEXT_10).font_weight(T::W_BOLD).text_color(T::text_muted()).child(title))
            .child(div().text_size(T::TEXT_10).font_weight(T::W_BOLD).text_color(T::text_secondary()).child(format!("({n})")))
            .child(div().flex_1())
            .child(w::icon_button(title, act, tip).on_click(cx.listener(move |g, _, _, cx| if staged { g.unstage_all(cx) } else { g.stage_all(cx) })))
            .into_any_element()
    }

    fn file_line(f: &FileRow, i: usize, cx: &mut Context<Self>) -> AnyElement {
        let code_color = match f.code.as_str() { "M" => T::yellow(), "A" | "?" => T::green(), "D" => T::red(), _ => T::text_secondary() };
        let (file, file2, file3) = (f.file.clone(), f.file.clone(), f.file.clone());
        let staged = f.staged;
        let id = (if staged { "staged" } else { "change" }, i);
        let mut acts = div().flex().flex_none().items_center().gap(T::SP_2)
            .child(w::icon_button((id.0, i * 4), IconName::GitCompare, "View diff").on_click(cx.listener(move |g, _, _, cx| g.diff(&file, staged, cx))));
        acts = if staged {
            acts.child(w::icon_button((id.0, i * 4 + 1), IconName::Minus, "Unstage").on_click(cx.listener(move |g, _, _, cx| g.unstage(&file2, cx))))
        } else {
            acts.child(w::icon_button((id.0, i * 4 + 1), IconName::Plus, "Stage").on_click(cx.listener(move |g, _, _, cx| g.stage(&file2, cx))))
                .child(w::icon_button((id.0, i * 4 + 2), IconName::RotateCcw, "Discard").on_click(cx.listener(move |g, _, window, cx| g.discard(&file3, window, cx))))
        };
        div().id(id).w_full().flex().items_center().gap(T::SP_6).h(T::CONTROL_SM).px(T::SP_10).hover(|s| s.bg(T::bg_hover()))
            .child(div().flex_none().w(T::ICON_MD).text_size(T::TEXT_11).font_weight(T::W_BOLD).font_family(T::MONO).text_color(code_color).child(f.code.clone()))
            .child(div().flex().flex_1().min_w_0().items_center().text_size(T::TEXT_12).font_family(T::MONO)
                .when(!f.dir.is_empty(), |d| d.child(div().text_color(T::text_muted()).truncate().child(f.dir.clone())))
                .child(div().text_color(T::text()).truncate().child(f.name.clone())))
            .when(!f.add.is_empty(), |d| d.child(div().text_size(T::TEXT_10).font_family(T::MONO).text_color(T::green()).child(format!("+{}", f.add))))
            .when(!f.del.is_empty(), |d| d.child(div().text_size(T::TEXT_10).font_family(T::MONO).text_color(T::red()).child(format!("-{}", f.del))))
            .child(acts)
            .into_any_element()
    }
}

impl Render for GitView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("git");
        let branch = if self.branch.is_empty() { "—".to_string() } else { self.branch.clone() };
        let head = w::toolbar()
            .child(div().text_size(T::ICON_MD).text_color(T::accent_hover()).child(IconName::GitBranch))
            .child(div().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).font_family(T::MONO).text_color(T::text()).child(branch))
            .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(self.sync.clone()))
            .child(div().flex_1())
            .child(w::icon_button("git-refresh", IconName::RefreshCw, "Refresh").on_click(cx.listener(|g, _, _, cx| g.load(cx))));
        let body: AnyElement = match self.state.as_str() {
            _ if self.project.is_empty() => w::empty("Select a project").into_any_element(),
            "loading" => w::loading("Loading…").into_any_element(),
            "not-repo" => w::empty("Not a git repository").into_any_element(),
            "error" => w::error_state("git-retry", format!("Error: {}", self.error), cx.listener(|g, _, _, cx| g.load(cx))).into_any_element(),
            _ => {
                let list: AnyElement = if self.rows.is_empty() {
                    div().flex_1().min_h_0().child(w::empty("No changes")).into_any_element()
                } else {
                    uniform_list("git-list", self.rows.len(), cx.processor(|g, range: std::ops::Range<usize>, _, cx| {
                        range.filter_map(|i| g.rows.get(i).cloned()).map(|l| match l {
                            Line::Section { staged, n } => Self::section(staged, n, cx),
                            Line::File(f, i) => Self::file_line(&f, i, cx),
                        }).collect::<Vec<_>>()
                    })).track_scroll(&self.list).w_full().flex_1().min_h_0().into_any_element()
                };
                let msg = self.msg.clone();
                let foot = div().flex().flex_col().flex_none().gap(T::SP_8).p(T::SP_10).border_t_1().border_color(T::border())
                    .child(w::input(&self.msg))
                    .child(div().flex().gap(T::SP_8)
                        .child(div().flex_1().child(w::disabled(w::button("git-pull", if self.pulling { "Pulling…" } else { "Pull" }).icon(IconName::ArrowDown), self.pulling).w_full().on_click(cx.listener(|g, _, _, cx| g.pull(cx)))))
                        .child(div().flex_1().child(w::disabled(w::button("git-push", if self.pushing { "Pushing…" } else { "Push" }).icon(IconName::ArrowUp), self.pushing).w_full().on_click(cx.listener(|g, _, _, cx| g.push(cx)))))
                        .child(div().flex_1().child(w::primary("git-commit", "Commit").w_full().on_click(cx.listener(move |g, _, window, cx| { let t = msg.read(cx).value().to_string(); g.commit(&t, window, cx) })))));
                div().flex().flex_col().flex_1().min_h_0().child(list).child(foot).into_any_element()
            }
        };
        div().flex().flex_col().size_full().bg(T::bg()).child(head).child(body)
    }
}

/// Make the view (once the window exists) and refresh it every 8 s while
/// the Git tab is on show (a refresh that finds nothing new draws nothing).
pub fn ensure(window: &mut Window, cx: &mut App) -> Entity<GitView> {
    if let Some(v) = view(cx) { return v; }
    let v = cx.new(|cx| GitView::new(window, cx));
    cx.set_global(GitHandle(v.clone()));
    v.update(cx, |g, cx| g.load(cx));
    cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor().timer(Duration::from_secs(8)).await;
        let _ = cx.update(|cx| {
            let tab = crundi_shell::stores(cx).nav.read(cx).tab.clone();
            if tab == "git" { if let Some(v) = view(cx) { v.update(cx, |g, cx| g.load(cx)); } }
        });
    }).detach();
    v
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that brings gpui's `test` attribute in over the std one.
    use super::{file_row, lines, sync_line, Line};
    use serde_json::json;

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

    #[test]
    fn sections() {
        let a = file_row(&json!({ "file": "a", "xy": "M ", "staged": true }));
        let b = file_row(&json!({ "file": "b", "xy": " M" }));
        let l = lines(&[a.clone()], &[b.clone()]);
        assert_eq!(l, vec![Line::Section { staged: true, n: 1 }, Line::File(a, 0), Line::Section { staged: false, n: 1 }, Line::File(b, 0)]);
        assert!(lines(&[], &[]).is_empty());
    }
}
