//! Git (phase 3.3; the web's loadGitInfo and gitPanelHtml, the Slint
//! client's src/workspace/git.rs): the project's branch with ↑ahead
//! ↓behind; Staged and Changes with their counts, Unstage all / Stage all;
//! each file's status letter, folder, name, +/− counts, View diff, Stage /
//! Unstage, Discard (asks first); the commit box with ↓ Pull, ↑ Push and
//! Commit. Refreshed every 8 s while the tab is on show.

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

pub struct GitView {
    pub state: String,
    pub error: String,
    pub info: Option<Value>,
    pub staged: Vec<FileRow>,
    pub changes: Vec<FileRow>,
    pub pulling: bool,
    pub pushing: bool,
    loading: bool,
    msg: Entity<w::InputState>,
    window: Option<AnyWindowHandle>,
}

pub struct GitHandle(pub Entity<GitView>);
impl Global for GitHandle {}

pub fn view(cx: &App) -> Option<Entity<GitView>> { cx.try_global::<GitHandle>().map(|h| h.0.clone()) }

impl GitView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let msg = cx.new(|cx| w::InputState::new(window, cx).placeholder("Commit message…"));
        GitView { state: "loading".into(), error: String::new(), info: None, staged: vec![], changes: vec![], pulling: false, pushing: false, loading: false, msg, window: Some(window.window_handle()) }
    }

    pub(crate) fn toast(&self, text: String, error: bool, cx: &mut App) {
        crate::toast(cx, text, if error { w::Toast::Error } else { w::Toast::Success });
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.info = None; self.state = "loading".into(); self.error.clear(); self.staged.clear(); self.changes.clear();
        cx.notify();
    }

    /// GET /api/git/info: the branch and the files; "Not a git repository"
    /// when the server says no.
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let project = crundi_shell::project(cx);
        if project.is_empty() || self.loading { return cx.notify(); }
        self.loading = true;
        let path = format!("/api/git/info?project={}", crate::enc(&project));
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
            let _ = me.update(cx, |g, cx| {
                g.loading = false;
                if crundi_shell::project(cx) != project { return; }
                match r {
                    Ok(v) if v.get("ok").is_some_and(truthy) => {
                        let files: Vec<Value> = v.get("files").and_then(|f| f.as_array()).cloned().unwrap_or_default();
                        g.staged = files.iter().filter(|f| f.get("staged").is_some_and(truthy)).map(file_row).collect();
                        g.changes = files.iter().filter(|f| !f.get("staged").is_some_and(truthy)).map(file_row).collect();
                        g.info = Some(v);
                        g.state = "ok".into();
                    }
                    Ok(_) => { g.info = None; g.state = "not-repo".into(); }
                    Err(e) => { g.state = "error".into(); g.error = e; }
                }
                cx.notify();
            });
        });
    }

    fn post_then_load(&mut self, path: &'static str, body: Value, fail: &'static str, cx: &mut Context<Self>) {
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.post(path, &body), move |r, cx| {
            let _ = me.update(cx, |g, cx| {
                match r {
                    Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => { let e = s(&v, "error"); g.toast(if e.is_empty() { fail.into() } else { e }, true, cx) }
                    Err(e) => g.toast(e, true, cx),
                    _ => {}
                }
                g.load(cx);
            });
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
        let win = self.window;
        crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| match r {
            Ok(v) if v.get("ok").is_some_and(truthy) => diffview::open(win, &project, &f, &s(&v, "old"), &s(&v, "new"), cx),
            _ => crate::toast(cx, "Failed to load diff".into(), w::Toast::Error),
        });
    }

    pub fn commit(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let msg = text.trim().to_string();
        // (Here the window is the one being updated: its toast directly.)
        if msg.is_empty() { return w::toast(window, cx, "Enter a commit message", w::Toast::Error); }
        let body = json!({ "project": crundi_shell::project(cx), "message": msg });
        let me = cx.entity().downgrade();
        let input = self.msg.clone();
        let win = self.window;
        crundi_shell::call(cx, move |api| api.post("/api/git/commit", &body), move |r, cx| {
            match r {
                Ok(v) if v.get("ok").is_some_and(truthy) => {
                    if let Some(h) = win { let _ = cx.update_window(h, |_, window, cx| input.update(cx, |st, cx| st.set_value("", window, cx))); }
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
            let _ = me.update(cx, |g, cx| {
                if push { g.pushing = false } else { g.pulling = false }
                let (done, fail) = if push { ("Pushed", "Push failed") } else { ("Pulled", "Pull failed") };
                match r {
                    Ok(v) if v.get("ok").is_some_and(truthy) => { g.toast(done.into(), false, cx); g.load(cx); }
                    Ok(v) => { let e = s(&v, "error"); g.toast(format!("{fail}: {}", if e.is_empty() { "unknown".into() } else { e }), true, cx) }
                    Err(e) => g.toast(format!("{fail}: {e}"), true, cx),
                }
                cx.notify();
            });
        });
    }
    pub fn push(&mut self, cx: &mut Context<Self>) { self.sync_op(true, cx) }
    pub fn pull(&mut self, cx: &mut Context<Self>) { self.sync_op(false, cx) }

    pub fn props(&self, cx: &App) -> Value {
        let state = if crundi_shell::project(cx).is_empty() { "none".to_string() } else { self.state.clone() };
        json!({
            "state": state,
            "branch": self.info.as_ref().map(|i| s(i, "branch")).unwrap_or_default(),
            "staged": self.staged.iter().map(|f| json!({ "file": f.file, "code": f.code })).collect::<Vec<_>>(),
            "changes": self.changes.iter().map(|f| json!({ "file": f.file, "code": f.code })).collect::<Vec<_>>(),
            "pulling": self.pulling, "pushing": self.pushing,
        })
    }

    // ─── Drawing ───

    fn section(&self, title: &'static str, n: usize, act: IconName, tip: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let staged = title == "STAGED";
        div().flex().items_center().gap(T::SP_6).h(T::CONTROL_SM).px(T::SP_10).bg(T::bg_secondary())
            .child(div().text_size(T::TEXT_10).font_weight(T::W_BOLD).text_color(T::text_muted()).child(title))
            .child(div().text_size(T::TEXT_10).font_weight(T::W_BOLD).text_color(T::text_secondary()).child(format!("({n})")))
            .child(div().flex_1())
            .child(w::icon_button(title, act, tip).on_click(cx.listener(move |g, _, _, cx| if staged { g.unstage_all(cx) } else { g.stage_all(cx) })))
    }

    fn file_line(&self, f: &FileRow, i: usize, cx: &mut Context<Self>) -> impl IntoElement {
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
        div().id(id).flex().items_center().gap(T::SP_6).h(T::CONTROL_SM).px(T::SP_10).hover(|s| s.bg(T::bg_hover()))
            .child(div().flex_none().w(T::ICON_MD).text_size(T::TEXT_11).font_weight(T::W_BOLD).font_family(T::MONO).text_color(code_color).child(f.code.clone()))
            .child(div().flex().flex_1().min_w_0().items_center().text_size(T::TEXT_12).font_family(T::MONO)
                .when(!f.dir.is_empty(), |d| d.child(div().text_color(T::text_muted()).truncate().child(f.dir.clone())))
                .child(div().text_color(T::text()).truncate().child(f.name.clone())))
            .when(!f.add.is_empty(), |d| d.child(div().text_size(T::TEXT_10).font_family(T::MONO).text_color(T::green()).child(format!("+{}", f.add))))
            .when(!f.del.is_empty(), |d| d.child(div().text_size(T::TEXT_10).font_family(T::MONO).text_color(T::red()).child(format!("-{}", f.del))))
            .child(acts)
    }
}

impl Render for GitView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.window.is_none() { self.window = Some(window.window_handle()); }
        let branch = self.info.as_ref().map(|i| s(i, "branch")).filter(|b| !b.is_empty()).unwrap_or_else(|| "—".into());
        let sync = self.info.as_ref().map(sync_line).unwrap_or_default();
        let head = w::toolbar()
            .child(div().text_size(T::ICON_MD).text_color(T::accent_hover()).child(IconName::GitBranch))
            .child(div().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).font_family(T::MONO).text_color(T::text()).child(branch))
            .child(div().text_size(T::TEXT_11).text_color(T::text_muted()).child(sync))
            .child(div().flex_1())
            .child(w::icon_button("git-refresh", IconName::RefreshCw, "Refresh").on_click(cx.listener(|g, _, _, cx| g.load(cx))));
        let body: AnyElement = match self.state.as_str() {
            _ if crundi_shell::project(cx).is_empty() => w::empty("Select a project").into_any_element(),
            "loading" => w::loading("Loading…").into_any_element(),
            "not-repo" => w::empty("Not a git repository").into_any_element(),
            "error" => w::error_state("git-retry", format!("Error: {}", self.error), cx.listener(|g, _, _, cx| g.load(cx))).into_any_element(),
            _ => {
                let mut list = div().id("git-list").flex().flex_col().flex_1().min_h_0().overflow_y_scroll();
                if self.staged.is_empty() && self.changes.is_empty() {
                    list = list.child(w::empty("No changes"));
                } else {
                    if !self.staged.is_empty() {
                        list = list.child(self.section("STAGED", self.staged.len(), IconName::Minus, "Unstage all", cx));
                        for (i, f) in self.staged.clone().iter().enumerate() { list = list.child(self.file_line(f, i, cx)); }
                    }
                    if !self.changes.is_empty() {
                        list = list.child(self.section("CHANGES", self.changes.len(), IconName::Plus, "Stage all", cx));
                        for (i, f) in self.changes.clone().iter().enumerate() { list = list.child(self.file_line(f, i, cx)); }
                    }
                }
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
/// the Git tab is on show.
pub fn ensure(window: &mut Window, cx: &mut App) -> Entity<GitView> {
    if let Some(v) = view(cx) { return v; }
    let v = cx.new(|cx| GitView::new(window, cx));
    cx.set_global(GitHandle(v.clone()));
    v.update(cx, |g, cx| g.load(cx));
    cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor().timer(Duration::from_secs(8)).await;
        let _ = cx.update(|cx| if crundi_shell::current_tab(cx) == "git" { if let Some(v) = view(cx) { v.update(cx, |g, cx| g.load(cx)); } });
    }).detach();
    v
}

#[cfg(test)]
mod tests {
    // Not `super::*`: that brings gpui's `test` attribute in over the std one.
    use super::{file_row, sync_line};
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
}
