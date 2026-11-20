//! Where the window is (the shell's store): the project tab on show, the
//! projects list folded or not (and its fold's ease), the browser drawer
//! open or not and its width.

use crundi_theme::T;
use gpui::*;
use std::time::Instant;

pub struct Nav {
    pub tab: String,
    pub folded: bool,
    /// The sidebar folding or unfolding: its width eases (the web's 220 ms):
    /// from, and since when.
    pub fold_from: Option<(Pixels, Instant)>,
    pub drawer_open: bool,
    pub drawer_w: Pixels,
    /// Docker on the server (GET /api/docker/status, once a sign-in):
    /// None until it answers; the tab only exists where it is installed.
    pub docker: Option<bool>,
}

impl Default for Nav {
    fn default() -> Self { Nav { tab: "workbench".into(), folded: false, fold_from: None, drawer_open: false, drawer_w: T::DRAWER_W, docker: None } }
}

/// The tabs that mean something with no project open (the web's
/// GLOBAL_TABS; its "widgets" is the Panels tab): the others are hidden then.
pub const GLOBAL_TABS: &[&str] = &["media", "mindmap", "panels", "docker", "homepool", "info", "secrets", "settings"];

/// Whether the tab bar shows `tab` (the web's syncTabScope and
/// checkDockerPresence): with no project, only the global tabs; Docker only
/// where it is installed (shown until the server has said).
pub fn tab_shown(tab: &str, project_open: bool, docker: Option<bool>) -> bool {
    (project_open || GLOBAL_TABS.contains(&tab)) && !(tab == "docker" && docker == Some(false))
}

impl Nav {
    /// The server's answer about Docker; away from a tab that is gone.
    pub fn set_docker(&mut self, installed: bool, cx: &mut Context<Self>) {
        if self.docker == Some(installed) { return; }
        self.docker = Some(installed);
        if !installed && self.tab == "docker" { self.tab = "info".into(); }
        cx.notify();
    }

    /// A project opened or none is (the web's syncTabScope): the tab bar
    /// shows its scope; `settled` (a project was closed, or the server has no
    /// project to open): stranded on a tab that just became meaningless,
    /// Info, which explains the state. (Not while the projects are still
    /// loading: the saved project opens then.)
    pub fn sync_scope(&mut self, project_open: bool, settled: bool, cx: &mut Context<Self>) {
        if settled && !tab_shown(&self.tab, project_open, self.docker) { self.tab = "info".into(); }
        cx.notify();
    }

    pub fn show_tab(&mut self, tab: &str, cx: &mut Context<Self>) {
        if self.tab != tab { self.tab = tab.to_string(); cx.notify(); }
    }

    pub fn set_drawer(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.drawer_open != open { self.drawer_open = open; cx.notify(); }
    }

    pub fn toggle_fold(&mut self, cx: &mut Context<Self>) {
        let from = self.sidebar_width().0;
        self.folded = !self.folded;
        self.fold_from = Some((from, Instant::now()));
        cx.notify();
    }

    /// The sidebar's width now, and whether it is still easing.
    pub fn sidebar_width(&self) -> (Pixels, bool) { sidebar_width(self.folded, self.fold_from) }
}

/// The sidebar's width now (folded or not, easing from `fold_from`), and
/// whether it is still easing.
pub fn sidebar_width(folded: bool, fold_from: Option<(Pixels, Instant)>) -> (Pixels, bool) {
    let to = if folded { T::SIDEBAR_FOLDED } else { T::SIDEBAR_W };
    let Some((from, at)) = fold_from else { return (to, false) };
    let t = (at.elapsed().as_secs_f32() / T::LAYOUT_EASE.as_secs_f32()).min(1.0);
    if t >= 1.0 { return (to, false); }
    let e = 1.0 - (1.0 - t).powi(3);
    (from + (to - from) * e, true)
}

#[cfg(test)]
mod tests {
    use super::tab_shown;

    #[test]
    fn the_tabs_follow_the_project_and_docker() {
        assert!(tab_shown("git", true, None));
        assert!(!tab_shown("git", false, None));
        assert!(!tab_shown("workbench", false, Some(true)));
        assert!(tab_shown("info", false, None));
        assert!(tab_shown("panels", false, None));
        assert!(!tab_shown("browsers", false, None));
        assert!(tab_shown("docker", true, None));
        assert!(tab_shown("docker", false, Some(true)));
        assert!(!tab_shown("docker", true, Some(false)));
    }
}
