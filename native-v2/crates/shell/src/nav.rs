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
}

impl Default for Nav {
    fn default() -> Self { Nav { tab: "workbench".into(), folded: false, fold_from: None, drawer_open: false, drawer_w: T::DRAWER_W } }
}

impl Nav {
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
    pub fn sidebar_width(&self) -> (Pixels, bool) {
        let to = if self.folded { T::SIDEBAR_FOLDED } else { T::SIDEBAR_W };
        let Some((from, at)) = self.fold_from else { return (to, false) };
        let t = (at.elapsed().as_secs_f32() / T::LAYOUT_EASE.as_secs_f32()).min(1.0);
        if t >= 1.0 { return (to, false); }
        let e = 1.0 - (1.0 - t).powi(3);
        (from + (to - from) * e, true)
    }
}
