//! The areas' hooks (AREAS.md, "How an area plugs in"). Each area registers
//! once, at start (`register`); the hooks are read in event handlers, the
//! live hub and the driver, never while rendering (ARCHITECTURE.md rule 1:
//! a Global holds hooks, never data that changes).

use crate::Stores;
use crundi_core::live::Event;
use crundi_theme::IconName;
use gpui::*;
use serde_json::Value;
use std::rc::Rc;

/// The project bar's tabs, in the web's order: (id, label, icon, the area that draws it).
pub const TABS: &[(&str, &str, IconName, &str)] = &[
    ("workbench", "Workbench", IconName::SquareTerminal, "workbench"),
    ("git", "Git", IconName::GitBranch, "workspace"),
    ("files", "Files", IconName::Folder, "workspace"),
    ("kanban", "Kanban", IconName::SquareKanban, "planning"),
    ("notes", "Notes", IconName::FileText, "planning"),
    ("mindmap", "Mindmap", IconName::Share2, "planning"),
    ("media", "Media", IconName::Images, "workspace"),
    ("panels", "Panels", IconName::LayoutDashboard, "operations"),
    ("factories", "Factories", IconName::Factory, "planning"),
    ("schedule", "Schedule", IconName::Clock, "planning"),
    ("services", "Services", IconName::Server, "operations"),
    ("docker", "Docker", IconName::Container, "operations"),
    ("homepool", "Homepool", IconName::House, "operations"),
    ("browsers", "Browsers", IconName::Globe, "desktop"),
    ("info", "Info", IconName::Info, "operations"),
    ("secrets", "Secrets", IconName::Lock, "operations"),
    ("settings", "Settings", IconName::Settings, "operations"),
];

pub type MakeTab = Rc<dyn Fn(&Stores, &mut Window, &mut App) -> AnyView>;
pub type OnBatch = Rc<dyn Fn(&Stores, &[Event], &mut App)>;
pub type OnText = Rc<dyn Fn(&Stores, &str, &mut App)>;
pub type OnUnit = Rc<dyn Fn(&Stores, &mut App)>;
pub type TestUi = Rc<dyn Fn(&Stores, &str, &[String], &mut Window, &mut App) -> Option<Value>>;
pub type State = Rc<dyn Fn(&Stores, &mut Window, &mut App) -> Value>;
pub type Action = Rc<dyn Fn(&Stores, &mut Window, &mut App)>;

/// One area's hooks.
#[derive(Clone)]
pub struct Area {
    pub name: &'static str,
    /// The tabs it draws: the tab's view, made once (when first shown).
    pub tabs: Vec<(&'static str, MakeTab)>,
    /// What the live hub took this frame (the whole batch, in order).
    pub on_batch: Option<OnBatch>,
    pub on_project: Option<OnText>,
    pub on_signed_in: Option<OnUnit>,
    pub on_signed_out: Option<OnUnit>,
    /// The driver's actions: Some(answer) for a name it knows. Unprefixed
    /// names go to every area in turn; "<area>:<name>" to that area only.
    pub test_ui: Option<TestUi>,
    /// Merged into the driver's "state".
    pub state: Option<State>,
    /// Named actions the shell's own buttons run ("add-menu", "browser", "cru"…).
    pub actions: Vec<(&'static str, Action)>,
}

impl Area {
    pub fn new(name: &'static str) -> Self {
        Area { name, tabs: vec![], on_batch: None, on_project: None, on_signed_in: None, on_signed_out: None, test_ui: None, state: None, actions: vec![] }
    }
    pub fn tab(mut self, id: &'static str, f: impl Fn(&Stores, &mut Window, &mut App) -> AnyView + 'static) -> Self { self.tabs.push((id, Rc::new(f))); self }
    pub fn on_batch(mut self, f: impl Fn(&Stores, &[Event], &mut App) + 'static) -> Self { self.on_batch = Some(Rc::new(f)); self }
    pub fn on_project(mut self, f: impl Fn(&Stores, &str, &mut App) + 'static) -> Self { self.on_project = Some(Rc::new(f)); self }
    pub fn on_signed_in(mut self, f: impl Fn(&Stores, &mut App) + 'static) -> Self { self.on_signed_in = Some(Rc::new(f)); self }
    pub fn on_signed_out(mut self, f: impl Fn(&Stores, &mut App) + 'static) -> Self { self.on_signed_out = Some(Rc::new(f)); self }
    pub fn test_ui(mut self, f: impl Fn(&Stores, &str, &[String], &mut Window, &mut App) -> Option<Value> + 'static) -> Self { self.test_ui = Some(Rc::new(f)); self }
    pub fn state(mut self, f: impl Fn(&Stores, &mut Window, &mut App) -> Value + 'static) -> Self { self.state = Some(Rc::new(f)); self }
    pub fn action(mut self, name: &'static str, f: impl Fn(&Stores, &mut Window, &mut App) + 'static) -> Self { self.actions.push((name, Rc::new(f))); self }
}

#[derive(Default)]
pub struct Areas(pub Vec<Area>);
impl Global for Areas {}

pub fn register(cx: &mut App, area: Area) {
    cx.default_global::<Areas>().0.push(area);
}

/// Every area (a copy of the hooks: a hook may register or read others).
pub fn list(cx: &App) -> Vec<Area> { cx.try_global::<Areas>().map(|a| a.0.clone()).unwrap_or_default() }

/// The maker of a tab's view, if an area draws it.
pub fn tab_maker(cx: &App, tab: &str) -> Option<MakeTab> {
    list(cx).into_iter().find_map(|a| a.tabs.iter().find(|(t, _)| *t == tab).map(|(_, f)| f.clone()))
}

/// Run a named action (false: nobody registered it).
pub fn run_action(stores: &Stores, name: &str, window: &mut Window, cx: &mut App) -> bool {
    let Some(f) = list(cx).into_iter().find_map(|a| a.actions.iter().find(|(n, _)| *n == name).map(|(_, f)| f.clone())) else { return false };
    f(stores, window, cx);
    true
}

pub fn tab_label(tab: &str) -> &'static str {
    TABS.iter().find(|t| t.0 == tab).map(|t| t.1).unwrap_or("This tab")
}
