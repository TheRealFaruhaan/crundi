//! What the screens (ported from native-gpui's operations crate) ask of the
//! shell, on v2's stores (AREAS.md: handles, read in handlers, never in a
//! view's render): the live batch as native-gpui's `AreaEvent`s, the
//! session's projects and terminals, the tab on show, a project picked.

use crundi_core::live::Event;
use gpui::*;
use serde_json::Value;

/// One live event as a screen hears it (native-gpui's `AreaEvent`).
pub enum AreaEvent<'a> {
    /// /ws and /api/events are up again (true) or lost.
    Connected(bool),
    /// The event stream's "state" (projects, terminals, services…): the
    /// last of the batch only (each is the whole state).
    State(&'a Value),
    /// Another /api/events event by name ("usage", "inbox", …).
    Server(&'a str, &'a Value),
    /// Another /ws message by type (the whole message).
    Ws(&'a str, &'a Value),
}

/// A frame's batch as the screens' events, in order; of the states only the
/// last is kept (ARCHITECTURE.md: the state event is the whole state).
pub fn events(batch: &[Event]) -> Vec<AreaEvent<'_>> {
    let last_state = batch.iter().rposition(|e| matches!(e, Event::State(_)));
    batch.iter().enumerate().filter_map(|(i, e)| Some(match e {
        Event::Connected(on) => AreaEvent::Connected(*on),
        Event::State(v) if Some(i) == last_state => AreaEvent::State(v),
        Event::Usage(v) => AreaEvent::Server("usage", v),
        Event::Inbox(v) => AreaEvent::Server("inbox", v),
        Event::ChatSecrets(v) => AreaEvent::Server("chat-secrets", v),
        Event::SecretRequests(v) => AreaEvent::Server("secret-requests", v),
        Event::Server(n, v) => AreaEvent::Server(n.as_str(), v),
        Event::Ws(t, v) => AreaEvent::Ws(t.as_str(), v),
        _ => return None,
    })).collect()
}

/// The session as the screens read it (in handlers).
pub mod session {
    use super::*;

    pub struct Snapshot { pub projects: Vec<Value>, pub terminals: Vec<Value>, pub online: bool, pub project: String, pub server: String }

    pub fn get(cx: &App) -> Snapshot {
        let s = crundi_shell::stores(cx);
        let p = s.projects.read(cx);
        let ss = s.session.read(cx);
        Snapshot { projects: p.raw().to_vec(), terminals: p.terminals.clone(), online: ss.online, project: p.open.clone(), server: ss.server.clone() }
    }

    /// Open a project (as the sidebar does); the areas hear it.
    pub fn select_project(cx: &mut App, alias: &str) {
        let s = crundi_shell::stores(cx);
        let a = alias.to_string();
        s.projects.update(cx, |p, cx| p.pick(&a, cx));
    }

    pub fn ws_send(cx: &App, msg: &Value) { crundi_shell::ws_send(cx, msg) }

    /// The sign-in screen's server step with this address (a restore that
    /// moved the server).
    pub fn check_server(cx: &mut App, base: &str) {
        let s = crundi_shell::stores(cx);
        let b = base.to_string();
        s.session.update(cx, |s, cx| s.check_server(&b, cx));
    }

    pub fn sign_out(cx: &mut App, why: &str) {
        let s = crundi_shell::stores(cx);
        let w = why.to_string();
        s.session.update(cx, |s, cx| s.sign_out(&w, cx));
    }
}

/// The tab on show.
pub fn current_tab(cx: &App) -> String { crundi_shell::stores(cx).nav.read(cx).tab.clone() }

/// Show a tab (deferred: a caller may be inside the shell's own update).
pub fn show_tab(cx: &mut App, tab: &str) {
    let t = tab.to_string();
    cx.defer(move |cx| { let s = crundi_shell::stores(cx); s.nav.update(cx, |n, cx| n.show_tab(&t, cx)); });
}

/// Settings' slots another area fills (Desktop: "voice-device", the
/// microphone and speaker of this device), drawn by the Settings page.
pub type SettingsPart = std::rc::Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
#[derive(Default)]
struct SettingsParts(Vec<(&'static str, SettingsPart)>);
impl Global for SettingsParts {}

/// Fill a Settings slot (at init).
pub fn register_settings_part(cx: &mut App, slot: &'static str, f: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) {
    cx.default_global::<SettingsParts>().0.push((slot, std::rc::Rc::new(f)));
}

/// What the parts put in `slot`, drawn now.
pub fn settings_parts(window: &mut Window, cx: &mut App, slot: &str) -> Vec<AnyElement> {
    let fs: Vec<SettingsPart> = cx.try_global::<SettingsParts>().map(|p| p.0.iter().filter(|(s, _)| *s == slot).map(|(_, f)| f.clone()).collect()).unwrap_or_default();
    fs.iter().map(|f| f(window, cx)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[::core::prelude::v1::test]
    fn only_the_last_state() {
        let b = vec![Event::State(json!({ "n": 1 })), Event::Connected(true), Event::State(json!({ "n": 2 })), Event::Server("widget".into(), json!({}))];
        let ev = events(&b);
        assert_eq!(ev.len(), 3);
        let states: Vec<i64> = ev.iter().filter_map(|e| if let AreaEvent::State(v) = e { v["n"].as_i64() } else { None }).collect();
        assert_eq!(states, vec![2]);
        assert!(matches!(ev[2], AreaEvent::Server("widget", _)));
    }
}
