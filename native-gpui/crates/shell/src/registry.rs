//! How a part plugs into the window (AREAS.md): each area registers, once at
//! start, its tabs (the tab host draws them), its handlers for the live link
//! and the project, and its test-driver actions, props and state. Only the
//! Lead changes this file.

use crundi_core::live::Event;
use gpui::{AnyElement, AnyView, App, Global, SharedString, Window};
use serde_json::{Map, Value};
use std::rc::Rc;

pub use crundi_theme::IconName;

/// The project bar's tabs, in the PC client's order, and which part draws
/// each (the web's .tab-btn list; Browsers is the server's browsers list).
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

/// What the live link tells the parts (on the UI thread).
pub enum AreaEvent<'a> {
    /// /ws and /api/events are up again (true) or lost: send subscriptions again.
    Connected(bool),
    /// A chat's ui-history or ui-event message.
    Chat(&'a Value),
    /// The event stream's "state" (projects, terminals, services…).
    State(&'a Value),
    /// Another /api/events event by name ("usage", "inbox", "chat-secrets", …).
    Server(&'a str, &'a Value),
    /// Another /ws message by type (the whole message).
    Ws(&'a str, &'a Value),
}

impl<'a> AreaEvent<'a> {
    pub fn from(e: &'a Event) -> Option<AreaEvent<'a>> {
        Some(match e {
            Event::Connected(on) => AreaEvent::Connected(*on),
            Event::Chat(v) => AreaEvent::Chat(v),
            Event::State(v) => AreaEvent::State(v),
            Event::Usage(v) => AreaEvent::Server("usage", v),
            Event::Inbox(v) => AreaEvent::Server("inbox", v),
            Event::ChatSecrets(v) => AreaEvent::Server("chat-secrets", v),
            Event::SecretRequests(v) => AreaEvent::Server("secret-requests", v),
            Event::Server(n, v) => AreaEvent::Server(n, v),
            Event::Ws(t, v) => AreaEvent::Ws(t, v),
            Event::SignedOut => return None,
        })
    }
}

type Render = Rc<dyn Fn(&str, &mut Window, &mut App) -> AnyElement>;
type OnEvent = Rc<dyn Fn(&AreaEvent, &mut App)>;
type OnText = Rc<dyn Fn(&str, &mut App)>;
type OnUnit = Rc<dyn Fn(&mut App)>;
type TestUi = Rc<dyn Fn(&str, &[String], &mut Window, &mut App) -> Option<Value>>;
type Props = Rc<dyn Fn(&mut Window, &mut App) -> Value>;

/// One part's hooks. Build with `Area::new("planning")` and the setters.
#[derive(Clone)]
pub struct Area {
    pub name: &'static str,
    /// The tab ids it draws (from TABS), and how.
    pub tabs: Vec<&'static str>,
    pub render: Option<Render>,
    pub on_event: Option<OnEvent>,
    pub on_project: Option<OnText>,
    pub on_signed_in: Option<OnUnit>,
    pub on_signed_out: Option<OnUnit>,
    pub on_tab: Option<OnText>,
    /// The driver's actions: Some(answer) for a name it knows. Unprefixed
    /// names ("send") are offered to every part in turn; "<area>:<name>" to that part only.
    pub test_ui: Option<TestUi>,
    /// Merged into the driver's "props" (top-level keys) and "state".
    pub props: Option<Props>,
    pub state: Option<Props>,
}

impl Area {
    pub fn new(name: &'static str) -> Area {
        Area { name, tabs: vec![], render: None, on_event: None, on_project: None, on_signed_in: None, on_signed_out: None, on_tab: None, test_ui: None, props: None, state: None }
    }
    pub fn tabs(mut self, tabs: &[&'static str], render: impl Fn(&str, &mut Window, &mut App) -> AnyElement + 'static) -> Area {
        self.tabs = tabs.to_vec();
        self.render = Some(Rc::new(render));
        self
    }
    pub fn on_event(mut self, f: impl Fn(&AreaEvent, &mut App) + 'static) -> Area { self.on_event = Some(Rc::new(f)); self }
    pub fn on_project(mut self, f: impl Fn(&str, &mut App) + 'static) -> Area { self.on_project = Some(Rc::new(f)); self }
    pub fn on_signed_in(mut self, f: impl Fn(&mut App) + 'static) -> Area { self.on_signed_in = Some(Rc::new(f)); self }
    pub fn on_signed_out(mut self, f: impl Fn(&mut App) + 'static) -> Area { self.on_signed_out = Some(Rc::new(f)); self }
    pub fn on_tab(mut self, f: impl Fn(&str, &mut App) + 'static) -> Area { self.on_tab = Some(Rc::new(f)); self }
    pub fn test_ui(mut self, f: impl Fn(&str, &[String], &mut Window, &mut App) -> Option<Value> + 'static) -> Area { self.test_ui = Some(Rc::new(f)); self }
    pub fn props(mut self, f: impl Fn(&mut Window, &mut App) -> Value + 'static) -> Area { self.props = Some(Rc::new(f)); self }
    pub fn state(mut self, f: impl Fn(&mut Window, &mut App) -> Value + 'static) -> Area { self.state = Some(Rc::new(f)); self }
}

/// Every registered part, and the browser drawer's view (Desktop's).
#[derive(Default)]
pub struct Areas {
    pub list: Vec<Area>,
    pub drawer: Option<Drawer>,
    /// The project bar's "+" by Workbench (the workbench's add menu).
    pub add_menu: Option<Rc<dyn Fn(&mut App)>>,
    /// A part's layer over the whole window, not modal (Desktop's Cru
    /// screen): it draws only what it shows, positioned absolutely.
    pub overlays: Vec<AnyView>,
}

impl Global for Areas {}

/// The drawer beside the project area (Desktop registers it).
#[derive(Clone)]
pub struct Drawer {
    pub view: AnyView,
    /// Pages are open (the project bar's Browser button shows its dot).
    pub has_pages: Rc<dyn Fn(&App) -> bool>,
}

/// Register a part (once, at start).
pub fn register(cx: &mut App, area: Area) {
    if !cx.has_global::<Areas>() { cx.set_global(Areas::default()); }
    cx.global_mut::<Areas>().list.push(area);
}

/// What the project bar's "+" by Workbench does (the workbench's, once:
/// the shell can't depend on the workbench).
pub fn register_add_menu(cx: &mut App, f: impl Fn(&mut App) + 'static) {
    if !cx.has_global::<Areas>() { cx.set_global(Areas::default()); }
    cx.global_mut::<Areas>().add_menu = Some(Rc::new(f));
}

/// The project bar's "+": the registered hook.
pub fn add_menu(cx: &mut App) {
    if let Some(f) = cx.try_global::<Areas>().and_then(|a| a.add_menu.clone()) { f(cx); }
}

/// Register a layer drawn over the whole window (a part's popover that is
/// not modal: Desktop's Cru screen). Added by Desktop, for the Lead to review.
pub fn register_overlay(cx: &mut App, view: AnyView) {
    if !cx.has_global::<Areas>() { cx.set_global(Areas::default()); }
    cx.global_mut::<Areas>().overlays.push(view);
}

/// The layers over the window, for the shell to draw.
pub(crate) fn overlays(cx: &App) -> Vec<AnyView> { cx.try_global::<Areas>().map(|a| a.overlays.clone()).unwrap_or_default() }

/// Register the browser drawer's view (Desktop, once).
pub fn register_drawer(cx: &mut App, drawer: Drawer) {
    if !cx.has_global::<Areas>() { cx.set_global(Areas::default()); }
    cx.global_mut::<Areas>().drawer = Some(drawer);
}

pub(crate) fn list(cx: &App) -> Vec<Area> {
    cx.try_global::<Areas>().map(|a| a.list.clone()).unwrap_or_default()
}

pub(crate) fn drawer(cx: &App) -> Option<Drawer> {
    cx.try_global::<Areas>().and_then(|a| a.drawer.clone())
}

/// The part that draws a tab.
pub(crate) fn render_tab(tab: &str, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let a = list(cx).into_iter().find(|a| a.tabs.contains(&tab))?;
    Some((a.render.as_ref()?)(tab, window, cx))
}

/// Tell every part (the live link, the project, sign-in, a tab).
pub fn each(cx: &mut App, f: impl Fn(&Area, &mut App)) {
    for a in list(cx) { f(&a, cx); }
}

/// The driver's action: a part's own ("<area>:<name>"), or the first part
/// that knows the name.
pub fn test_ui(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    let areas = list(cx);
    if let Some((area, rest)) = name.split_once(':') {
        let a = areas.iter().find(|a| a.name == area)?;
        return (a.test_ui.as_ref()?)(rest, args, window, cx);
    }
    for a in &areas {
        if let Some(f) = &a.test_ui { if let Some(v) = f(name, args, window, cx) { return Some(v); } }
    }
    None
}

/// Every part's props (or state), merged into one object.
pub fn merged(window: &mut Window, cx: &mut App, state: bool) -> Map<String, Value> {
    let mut out = Map::new();
    for a in list(cx) {
        let f = if state { a.state.clone() } else { a.props.clone() };
        if let Some(f) = f {
            if let Value::Object(m) = f(window, cx) { out.extend(m); }
        }
    }
    out
}

/// One part's props alone ({"cmd":"area"}).
pub fn props_of(name: &str, window: &mut Window, cx: &mut App) -> Value {
    let a = list(cx).into_iter().find(|a| a.name == name);
    a.and_then(|a| a.props.map(|f| f(window, cx))).unwrap_or(Value::Null)
}

/// A tab's label (for placeholders and tests).
pub fn tab_label(id: &str) -> SharedString {
    TABS.iter().find(|t| t.0 == id).map(|t| t.1).unwrap_or(id).into()
}
