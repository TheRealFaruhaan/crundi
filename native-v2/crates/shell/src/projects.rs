//! The projects store (the shell's): the projects, the open one, the
//! event stream's terminals (chats and Claude terminals) and each project's
//! dot, and the projects' icons. It changes from the live hub (the "state"
//! event, at most once a frame) and from a pick in the sidebar; it notifies
//! only when what it holds changed.

use crundi_core::{head, store};
use gpui::*;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub alias: String,
    pub name: String,
    /// GET /api/project-icon?… (the server's pick of the repo's icon), or "".
    pub icon: String,
    /// The sidebar's dot: needs-input | working | waiting | running | "".
    pub dot: &'static str,
}

/// The open project changed (the areas hear it once the update is over).
pub struct ProjectPicked(pub String);

#[derive(Default)]
pub struct Projects {
    pub list: Vec<Project>,
    pub open: String,
    /// The event stream's last terminals.
    pub terminals: Vec<Value>,
    raw: Vec<Value>,
    /// The icons that have arrived, by their address.
    icons: HashMap<String, Option<Arc<Image>>>,
}
impl EventEmitter<ProjectPicked> for Projects {}

impl Projects {
    /// The open project's name ("" for none).
    pub fn open_name(&self) -> String {
        self.list.iter().find(|p| p.alias == self.open).map(|p| p.name.clone()).unwrap_or_default()
    }

    /// The picture of a project's icon, if it has arrived.
    pub fn icon_of(&self, alias: &str) -> Option<Arc<Image>> {
        let p = self.list.iter().find(|p| p.alias == alias)?;
        self.icons.get(&p.icon).cloned().flatten()
    }

    fn derive(&mut self) -> Vec<Project> {
        self.raw.iter().map(|p| {
            let alias = p.get("alias").and_then(|a| a.as_str()).unwrap_or("").to_string();
            let name = p.get("name").and_then(|n| n.as_str()).filter(|n| !n.is_empty()).unwrap_or(&alias).to_string();
            let icon = p.get("icon").and_then(|u| u.as_str()).unwrap_or("").to_string();
            let dot = head::project_dot(&self.terminals, &alias);
            Project { alias, name, icon, dot }
        }).collect()
    }

    /// The projects list (GET /api/projects, or the state event's).
    pub fn set_projects(&mut self, raw: Vec<Value>, cx: &mut Context<Self>) {
        if raw == self.raw { return; }
        self.raw = raw;
        self.rederive(cx);
    }

    /// The event stream's state: its projects (when it has them) and terminals.
    pub fn apply_state(&mut self, st: &Value, cx: &mut Context<Self>) {
        let mut changed = false;
        if let Some(p) = st.get("projects").and_then(|p| p.as_array()) { if !p.is_empty() && *p != self.raw { self.raw = p.clone(); changed = true; } }
        if let Some(t) = st.get("terminals").and_then(|t| t.as_array()) {
            // A chat's title, long title or state is read from the terminals
            // (the heads, through their `watch`): a change there notifies
            // even when the derived list stays the same (F29: a rename came
            // in and no head redrew). Observers compare what they show.
            if *t != self.terminals { self.terminals = t.clone(); changed = true; cx.notify(); }
        }
        if changed { self.rederive(cx); }
    }

    fn rederive(&mut self, cx: &mut Context<Self>) {
        let list = self.derive();
        if list != self.list { self.list = list; cx.notify(); }
        self.fetch_icons(cx);
    }

    /// Open a project (the sidebar, the driver); the areas hear it.
    pub fn pick(&mut self, alias: &str, cx: &mut Context<Self>) {
        if self.open == alias { return; }
        self.open = alias.to_string();
        store::update(|s| s.project = alias.to_string());
        cx.emit(ProjectPicked(alias.to_string()));
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        *self = Projects::default();
        cx.notify();
    }

    /// Each project's icon, fetched once (the web's projIconSrc); the project
    /// keeps its dot and letters until it arrives, and for good if it can't.
    fn fetch_icons(&mut self, cx: &mut Context<Self>) {
        let want: Vec<String> = self.list.iter().map(|p| p.icon.clone()).filter(|u| u.starts_with("/api/project-icon?") && !self.icons.contains_key(u)).collect();
        for url in want {
            self.icons.insert(url.clone(), None);
            let Some(api) = crate::api(cx) else { return };
            let u = url.clone();
            let task = cx.background_spawn(async move { api.get_bytes(&u).ok() });
            cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                let Some((200, bytes)) = task.await else { return };
                let Some(format) = sniff(&bytes) else { return };
                let _ = this.update(cx, |p, cx| { p.icons.insert(url, Some(Arc::new(Image::from_bytes(format, bytes)))); cx.notify(); });
            }).detach();
        }
    }
}

/// The picture's format from its first bytes.
fn sniff(b: &[u8]) -> Option<ImageFormat> {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) { return Some(ImageFormat::Png); }
    if b.starts_with(&[0xFF, 0xD8]) { return Some(ImageFormat::Jpeg); }
    if b.starts_with(b"GIF8") { return Some(ImageFormat::Gif); }
    if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" { return Some(ImageFormat::Webp); }
    if b.starts_with(&[0, 0, 1, 0]) { return Some(ImageFormat::Ico); }
    if b.starts_with(b"BM") { return Some(ImageFormat::Bmp); }
    let head = String::from_utf8_lossy(&b[..b.len().min(512)]).to_ascii_lowercase();
    if head.contains("<svg") { return Some(ImageFormat::Svg); }
    None
}
