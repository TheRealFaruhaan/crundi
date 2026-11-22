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
    /// Its registered services: (running, all); None when it has none (the heartbeat).
    pub heart: Option<(usize, usize)>,
    /// It has upcoming scheduled tasks (the clock).
    pub sched: bool,
    /// A factory running in it: (its name, it needs you, its status).
    pub factory: Option<(String, bool, String)>,
}

impl Project {
    /// The heartbeat's tooltip (renderProjects).
    pub fn heart_tip(&self) -> String {
        match self.heart {
            Some((r, n)) if r > 0 => format!("{r} of {n} service(s) running"),
            Some((_, n)) => format!("{n} service(s), all stopped"),
            None => String::new(),
        }
    }
    /// The factory mark's tooltip.
    pub fn factory_tip(&self) -> String {
        self.factory.as_ref().map(|(n, needs, st)| format!("Factory {n}: {}", if *needs { "needs you" } else { st })).unwrap_or_default()
    }
}

/// A factory run needs the owner (the web's factoryNeedsYou).
pub fn factory_needs_you(r: &Value) -> bool {
    let st = r.get("status").and_then(|s| s.as_str()).unwrap_or("");
    let waiting = r.get("waitingOn").is_some_and(|w| !(w.is_null() || *w == Value::Bool(false) || w.as_str() == Some("")));
    matches!(st, "crashed" | "finishing" | "waiting") || waiting
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
    /// Who is signed in: "owner" or "collaborator" (the state's role).
    pub role: String,
    /// "multi" (projects added by hand: they can be removed) or "single"
    /// (GET /api/projects/config).
    pub mode: String,
    services: Vec<Value>,
    scheduled: Vec<String>,
    factory_runs: Vec<Value>,
    /// The icons that have arrived, by their address.
    icons: HashMap<String, Option<Arc<Image>>>,
}
impl EventEmitter<ProjectPicked> for Projects {}

impl Projects {
    /// The open project's name ("" for none).
    pub fn open_name(&self) -> String {
        self.list.iter().find(|p| p.alias == self.open).map(|p| p.name.clone()).unwrap_or_default()
    }

    /// The projects as the server sent them (Operations: a project's settings, Homepool's lists).
    pub fn raw(&self) -> &[Value] { &self.raw }

    /// A project's folder on the server ("" when unknown).
    pub fn path_of(&self, alias: &str) -> String {
        self.raw.iter().find(|p| p.get("alias").and_then(|a| a.as_str()) == Some(alias)).and_then(|p| p.get("path").and_then(|v| v.as_str())).unwrap_or("").to_string()
    }

    /// Every project's alias and folder (a path from a chat finds its project).
    pub fn paths(&self) -> Vec<(String, String)> {
        self.raw.iter().map(|p| (p.get("alias").and_then(|a| a.as_str()).unwrap_or("").to_string(), p.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string())).collect()
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
            let low = alias.to_lowercase();
            let of = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let svcs: Vec<&Value> = self.services.iter().filter(|s| of(s, "alias").to_lowercase() == low).collect();
            let heart = (!svcs.is_empty()).then(|| (svcs.iter().filter(|s| of(s, "status") == "running").count(), svcs.len()));
            let sched = self.scheduled.iter().any(|x| *x == low);
            let factory = self.factory_runs.iter().find(|r| of(r, "project").to_lowercase() == low).map(|r| (of(r, "factoryName"), factory_needs_you(r), of(r, "status")));
            Project { alias, name, icon, dot, heart, sched, factory }
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
        // The sidebar's marks: services (the heartbeat), schedules, factory runs.
        if let Some(v) = st.get("services").and_then(|x| x.as_array()) { if *v != self.services { self.services = v.clone(); changed = true; } }
        if let Some(v) = st.get("scheduled").and_then(|x| x.as_array()) {
            let v: Vec<String> = v.iter().filter_map(|x| x.as_str().map(|s| s.to_lowercase())).collect();
            if v != self.scheduled { self.scheduled = v; changed = true; }
        }
        if let Some(v) = st.get("factoryRuns").and_then(|x| x.as_array()) { if *v != self.factory_runs { self.factory_runs = v.clone(); changed = true; } }
        if let Some(r) = st.get("role").and_then(|x| x.as_str()) { if r != self.role { self.role = r.to_string(); cx.notify(); } }
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

    /// How many icons have arrived (a view drawing them redraws when one did).
    pub fn icons_ready(&self) -> usize { self.icons.values().filter(|i| i.is_some()).count() }

    /// Who is signed in is a collaborator (the web's userRole).
    pub fn collaborator(&self) -> bool { self.role == "collaborator" }

    /// The project mode (GET /api/projects/config).
    pub fn set_mode(&mut self, mode: &str, cx: &mut Context<Self>) {
        if self.mode != mode { self.mode = mode.to_string(); cx.notify(); }
    }

    /// The projects in a new order (a drag in the sidebar), at once; the
    /// caller tells the server.
    pub fn reorder(&mut self, order: &[String], cx: &mut Context<Self>) {
        let pos = |v: &Value| { let a = v.get("alias").and_then(|a| a.as_str()).unwrap_or(""); order.iter().position(|x| x == a).unwrap_or(usize::MAX) };
        self.raw.sort_by_key(pos);
        self.rederive(cx);
    }

    /// A project removed (the server said so): gone from the list; the open
    /// one closes ("No project").
    pub fn removed(&mut self, alias: &str, cx: &mut Context<Self>) {
        self.raw.retain(|p| p.get("alias").and_then(|a| a.as_str()) != Some(alias));
        if self.open == alias { self.open.clear(); store::update(|s| s.project.clear()); cx.emit(ProjectPicked(String::new())); }
        self.rederive(cx);
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

#[cfg(test)]
mod tests {
    use super::Projects;
    use gpui::{AppContext as _, TestAppContext};
    use serde_json::json;

    #[gpui::test]
    fn the_rows_marks_follow_the_state(cx: &mut TestAppContext) {
        let p = cx.new(|_| Projects::default());
        p.update(cx, |p, cx| p.apply_state(&json!({
            "projects": [{ "alias": "demo", "name": "Demo" }, { "alias": "Other", "name": "Other" }, { "alias": "z" }],
            "role": "collaborator",
            "services": [{ "alias": "DEMO", "status": "running" }, { "alias": "demo", "status": "stopped" }, { "alias": "other", "status": "stopped" }],
            "scheduled": ["other"],
            "factoryRuns": [{ "project": "demo", "factoryName": "Ship", "status": "running" }, { "project": "other", "factoryName": "Fix", "status": "running", "waitingOn": "you" }],
        }), cx));
        p.read_with(cx, |p, _| {
            let (d, o, z) = (&p.list[0], &p.list[1], &p.list[2]);
            assert_eq!(d.heart, Some((1, 2)));
            assert_eq!(d.heart_tip(), "1 of 2 service(s) running");
            assert_eq!(o.heart_tip(), "1 service(s), all stopped");
            assert_eq!(z.heart, None);
            assert!(!d.sched && o.sched);
            assert_eq!(d.factory_tip(), "Factory Ship: running");
            assert_eq!(o.factory_tip(), "Factory Fix: needs you");
            assert!(p.collaborator());
        });
        // A drag's new order; a removal (of a project not open: no saved login touched).
        p.update(cx, |p, cx| p.reorder(&["z".into(), "demo".into(), "Other".into()], cx));
        p.read_with(cx, |p, _| assert_eq!(p.list.iter().map(|x| x.alias.as_str()).collect::<Vec<_>>(), ["z", "demo", "Other"]));
        p.update(cx, |p, cx| p.removed("demo", cx));
        p.read_with(cx, |p, _| assert_eq!(p.list.len(), 2));
    }
}
