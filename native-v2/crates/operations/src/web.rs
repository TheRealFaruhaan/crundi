//! The browser Panels and What's new draw web pages with (the Desktop area's
//! off-screen CEF host, `crates/webview`): a hook the Desktop area sets at
//! init (`set_host`), as native-gpui's crundi_webview `engine` (Panels: one
//! engine panel per copy on screen, drawn by its page view) and `frames`
//! (What's new: a step's page as a picture). Until a host is set, nothing
//! is started: Panels say the browser did not start, What's new shows each
//! step's title, as native-gpui does without CEF.
//!
//! A Global holds only this hook (ARCHITECTURE.md rule 1), read in handlers.

use gpui::*;
use std::{rc::Rc, sync::Arc, time::Duration};

/// One engine panel's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PanelState {
    /// Open, with a browser.
    pub live: bool,
    /// Open and has painted at least one frame.
    pub painted: bool,
    /// The page's title (Panels' bridge reads the runtime's messages there).
    pub title: String,
}

/// A view drawing an engine panel (made per copy on screen). The host's view
/// redraws itself as its panel paints (at most once a frame, only while it
/// is drawn); Panels only tells it which panel to show.
#[derive(Clone)]
pub struct PageView {
    pub view: AnyView,
    pub set_panel: Rc<dyn Fn(Option<usize>, &mut App)>,
}

pub trait Host {
    /// The browser host started (or can start).
    fn started(&self) -> bool;
    /// How long a page not drawn keeps its browser.
    fn unload_grace(&self) -> Duration;
    // Engine panels (Panels).
    fn panel_open(&self, url: &str) -> usize;
    fn panel_navigate(&self, i: usize, url: &str);
    fn panel_close(&self, i: usize);
    fn panel_resized(&self, i: usize, w: f32, h: f32);
    fn panel_parked(&self, i: usize, parked: bool);
    fn panel_state(&self, i: usize) -> PanelState;
    fn page_view(&self, cx: &mut App) -> PageView;
    // Frames (What's new): a page drawn as a picture.
    fn frame_open(&self, key: &str, url: &str, w: f32, h: f32, scale: f32) -> bool;
    fn frame_resize(&self, key: &str, w: f32, h: f32, scale: f32);
    fn frame_close(&self, key: &str);
    fn frame_exists(&self, key: &str) -> bool;
    /// Bumped whenever any frame paints.
    fn frame_generation(&self) -> u64;
    fn frame_failed(&self, key: &str) -> bool;
    fn frame_painted(&self, key: &str) -> bool;
    fn frame_crashed(&self, key: &str) -> bool;
    fn frame_image(&self, key: &str) -> Option<Arc<RenderImage>>;
    /// The address it was opened at, its page's title, how many loads (the driver's).
    fn frame_url(&self, key: &str) -> String;
    fn frame_title(&self, key: &str) -> String;
    fn frame_loads(&self, key: &str) -> u64;
    fn crash_overlay(&self, key: &str) -> AnyElement;
}

struct HostHook(Rc<dyn Host>);
impl Global for HostHook {}

/// The Desktop area's browser (at init).
pub fn set_host(cx: &mut App, host: Rc<dyn Host>) { cx.set_global(HostHook(host)); }

pub(crate) fn host(cx: &App) -> Option<Rc<dyn Host>> { cx.try_global::<HostHook>().map(|h| h.0.clone()) }

pub(crate) fn started(cx: &App) -> bool { host(cx).is_some_and(|h| h.started()) }

pub(crate) fn unload_grace(cx: &App) -> Duration { host(cx).map(|h| h.unload_grace()).unwrap_or(Duration::from_secs(5)) }
