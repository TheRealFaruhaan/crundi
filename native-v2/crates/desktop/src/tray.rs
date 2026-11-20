//! The tray (specs/desktop/inbox-tray-bubble.md A.2-A.3; app/main.js
//! createTray, updateTrayMenu): the head icon, the tooltip "Crundi —
//! <status>", and the menu in the PC client's order and wording, as far as
//! it applies to this client: no Start/Stop (no bundled server: A.3's note),
//! no updates (phase 7). The PC lock's "Lock (visible)" and "Lock (full)".
//! Kid mode as the PC client's: "Locked", the locks and Quit off while the
//! window is locked (kid mode or the PC lock: Electron's kidLocked). "Hey Cru" (voiceWakeTrayItems)
//! with its note. A click (Linux; double click elsewhere) shows the window.
//!
//! Linux: a StatusNotifierItem (ksni, over the session D-Bus). Windows and
//! macOS: tray-icon. Either way a pick is queued on the tray's own thread
//! (or the platform's event handler), which wakes the area's pump
//! (`crundi_webview::wake`): nothing polls for picks. The pump acts on them
//! with the window. No tray (no session bus, no watcher) is logged and the
//! client carries on (A.2).

use crate::ctx;
use serde_json::{json, Value};
use std::sync::Mutex;

/// The head drawn for small sizes (A.2: icon_32x32.png).
const ICON_PNG: &[u8] = include_bytes!("../../../../assets/icon_32x32.png");

/// What the menu shows: the connection, the kid screen's lock, "Hey Cru".
#[derive(Clone, PartialEq, Debug, Default)]
pub struct State {
    pub status: String,
    pub kid_locked: bool,
    /// "Hey Cru": (on, trained, its note).
    pub wake: Option<(bool, bool, String)>,
}

/// One item: (id, label, enabled, checked: Some for a check item); "-" is a separator.
pub type Item = (&'static str, String, bool, Option<bool>);

/// The wake setting's words (voiceWakeTrayItems).
pub const WAKE_LABEL: &str = "Listen for \"Hey Cru\" while plugged in";

/// The menu (A.3; updateTrayMenu), in the PC client's order and words.
pub fn menu(s: &State) -> Vec<Item> {
    let sep = || ("-", String::new(), false, None);
    let mut m = vec![
        ("head", "Crundi".into(), false, None),
        sep(),
        ("status", s.status.clone(), false, None),
        sep(),
        ("open", "Open".into(), true, None),
        ("lock-visible", "Lock (visible)".into(), !s.kid_locked, None),
        ("lock-full", "Lock (full)".into(), !s.kid_locked, None),
        ("kid", if s.kid_locked { "Locked".into() } else { "Kid mode".into() }, !s.kid_locked, None),
        ("server", "Server\u{2026}".into(), true, None),
        sep(),
    ];
    if let Some((on, trained, note)) = &s.wake {
        m.push(("wake", WAKE_LABEL.into(), *trained, Some(*on)));
        if !trained { m.push(("wake-note", "    Train it first in Settings, Voice assistant".into(), false, None)); }
        else if !note.is_empty() { m.push(("wake-note", format!("    {note}"), false, None)); }
        m.push(sep());
    }
    m.push(("quit", "Quit".into(), !s.kid_locked, None));
    m
}

/// A.3's status labels, from the connection: running, or stopped.
pub fn status_label(online: bool) -> &'static str { if online { "Running" } else { "Stopped" } }

/// "Crundi — <status>" (an em dash).
pub fn tooltip(status: &str) -> String { format!("Crundi \u{2014} {status}") }

static PICKS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// (the tray is up, what it shows).
static SHOWN: Mutex<Option<(bool, State)>> = Mutex::new(None);

/// A pick, from the tray's thread: queued, and the pump woken for it.
fn pick(id: &str) { PICKS.lock().unwrap().push(id.to_string()); crundi_webview::wake(); }

/// The tray's picks since the last pump.
pub fn take_picks() -> Vec<String> { std::mem::take(&mut *PICKS.lock().unwrap()) }

/// Picks put back for the pump with the window (opened again for them).
pub fn requeue(picks: Vec<String>) { if picks.is_empty() { return; } let mut g = PICKS.lock().unwrap(); for (i, p) in picks.into_iter().enumerate() { g.insert(i, p); } }

/// Act on a pick (the area's pump, with the window).
pub fn act(id: &str) {
    let locked = crate::cru_kid::on() || crate::pclock::on();
    match id {
        "open" => ctx::raise(),
        "lock-visible" if !locked => crate::pclock::ask("visible", "tray"),
        "lock-full" if !locked => crate::pclock::ask("full", "tray"),
        // A.3's "Kid mode": started here (the kid screen opens on this PC).
        "kid" if !locked => { ctx::raise(); crate::cru_kid::start("tray"); }
        // The server's settings (the PC client's "Server…" shows its server page).
        "server" => { ctx::raise(); ctx::show_tab("settings"); }
        "wake" => { let on = crate::wake::status()["enabled"].as_bool().unwrap_or(false); crate::wake::set(!on); }
        // Not while the kid screen is locked: only the PIN ends it.
        "quit" if !locked => ctx::on_app(|_, cx| cx.quit()),
        _ => {}
    }
    refresh();
}

/// The icon as RGBA, its size.
fn icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let i = image::load_from_memory(ICON_PNG).ok()?.to_rgba8();
    let (w, h) = i.dimensions();
    Some((i.into_raw(), w, h))
}

/// What the menu shows now, from the logic (cheap reads).
fn now(status: String) -> State {
    let w = crate::wake::status();
    let b = |k: &str| w[k].as_bool().unwrap_or(false);
    State { status, kid_locked: crate::cru_kid::on() || crate::pclock::on(), wake: Some((b("enabled"), b("trained"), w["label"].as_str().filter(|_| b("enabled")).unwrap_or("").to_string())) }
}

/// Make the tray (once, at start).
pub fn start(online: bool) {
    let s = now(status_label(online).to_string());
    let ok = platform::start(&s);
    *SHOWN.lock().unwrap() = Some((ok, s));
}

/// The menu follows what it shows (each pump: a compare; the tray is told
/// only of a change).
pub fn refresh() {
    let Some(cur) = SHOWN.lock().unwrap().as_ref().map(|x| x.1.status.clone()) else { return };
    set(now(cur));
}

fn set(s: State) {
    let mut g = SHOWN.lock().unwrap();
    let Some((ok, cur)) = g.as_mut() else { return };
    if *cur == s { return; }
    *cur = s.clone();
    if *ok { platform::update(&s); }
}

/// The tray is up (the window's × hides to it then).
pub fn up() -> bool { SHOWN.lock().unwrap().as_ref().is_some_and(|x| x.0) }

/// The connection changed: the status line and the tooltip follow.
pub fn set_online(online: bool) { set(now(status_label(online).to_string())); }

/// What the test driver sees: shown, the tooltip, the menu's labels.
pub fn props() -> Value {
    let s = SHOWN.lock().unwrap().clone();
    let (shown, st) = s.unwrap_or_default();
    json!({
        "shown": shown, "tooltip": tooltip(&st.status), "hidden": crate::ctx::hidden(),
        "menu": menu(&st).iter().map(|(id, l, en, ch)| json!({ "id": id, "label": l, "enabled": en, "checked": ch })).collect::<Vec<_>>(),
    })
}

/// A pick as the tray's menu makes it (the test driver's "tray-pick").
pub fn test_pick(id: &str) { pick(id); }

#[cfg(target_os = "linux")]
mod platform {
    use ksni::blocking::{Handle, TrayMethods};
    use std::sync::Mutex;

    struct Tray { state: super::State, icon: Vec<ksni::Icon> }

    impl ksni::Tray for Tray {
        // v2's own id: never taken for the Electron app's or native-gpui's item.
        fn id(&self) -> String { "crundi-v2".into() }
        fn title(&self) -> String { "Crundi".into() }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> { self.icon.clone() }
        fn tool_tip(&self) -> ksni::ToolTip { ksni::ToolTip { title: super::tooltip(&self.state.status), ..Default::default() } }
        // Linux: a single click shows the window too (A.2).
        fn activate(&mut self, _x: i32, _y: i32) { super::pick("open"); }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::*;
            super::menu(&self.state).into_iter().map(|(id, label, enabled, check)| {
                if id == "-" { return MenuItem::Separator; }
                match check {
                    Some(checked) => CheckmarkItem { label, enabled, checked, activate: Box::new(move |_: &mut Self| super::pick(id)), ..Default::default() }.into(),
                    None => StandardItem { label, enabled, activate: Box::new(move |_: &mut Self| super::pick(id)), ..Default::default() }.into(),
                }
            }).collect()
        }
    }

    static HANDLE: Mutex<Option<Handle<Tray>>> = Mutex::new(None);

    pub fn start(state: &super::State) -> bool {
        // ARGB32, network byte order.
        let icon = super::icon_rgba().map(|(rgba, w, h)| {
            let data = rgba.chunks_exact(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
            vec![ksni::Icon { width: w as i32, height: h as i32, data }]
        }).unwrap_or_default();
        // Registered even before a panel's watcher is there (it finds the item when it comes).
        match (Tray { state: state.clone(), icon }).assume_sni_available(true).spawn() {
            Ok(h) => { *HANDLE.lock().unwrap() = Some(h); true }
            Err(e) => { eprintln!("[lifecycle] Tray error: {e}"); false }
        }
    }

    pub fn update(state: &super::State) {
        let s = state.clone();
        if let Some(h) = HANDLE.lock().unwrap().as_ref() { h.update(move |t| t.state = s); }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
use other as platform;

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod other {
    use std::cell::RefCell;
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

    thread_local! { static TRAY: RefCell<Option<TrayIcon>> = const { RefCell::new(None) }; }

    fn build(state: &super::State) -> Menu {
        let menu = Menu::new();
        for (id, label, enabled, check) in super::menu(state) {
            if id == "-" { let _ = menu.append(&PredefinedMenuItem::separator()); continue; }
            let _ = match check {
                Some(on) => menu.append(&CheckMenuItem::with_id(id, &label, enabled, on, None)),
                None => menu.append(&MenuItem::with_id(id, &label, enabled, None)),
            };
        }
        menu
    }

    pub fn start(state: &super::State) -> bool {
        // The platform's events wake the pump from its own loop: never polled.
        MenuEvent::set_event_handler(Some(|e: MenuEvent| super::pick(&e.id().0)));
        TrayIconEvent::set_event_handler(Some(|e: TrayIconEvent| {
            if let TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } = e { super::pick("open"); }
        }));
        let mut b = TrayIconBuilder::new().with_tooltip(super::tooltip(&state.status)).with_menu(Box::new(build(state)));
        if let Some((rgba, w, h)) = super::icon_rgba() {
            if let Ok(i) = tray_icon::Icon::from_rgba(rgba, w, h) { b = b.with_icon(i); }
        }
        match b.build() {
            Ok(t) => { TRAY.with(|x| *x.borrow_mut() = Some(t)); true }
            Err(e) => { eprintln!("[lifecycle] Tray error: {e}"); false }
        }
    }

    pub fn update(state: &super::State) {
        TRAY.with(|x| if let Some(t) = x.borrow().as_ref() {
            let _ = t.set_tooltip(Some(super::tooltip(&state.status)));
            t.set_menu(Some(Box::new(build(state))));
        });
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
mod platform {
    pub fn start(_state: &super::State) -> bool { false }
    pub fn update(_state: &super::State) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(s: &State) -> Vec<String> { menu(s).into_iter().map(|x| x.1).collect() }

    #[test]
    fn the_menu_and_its_words() {
        let s = State { status: "Running".into(), ..Default::default() };
        assert_eq!(labels(&s), ["Crundi", "", "Running", "", "Open", "Lock (visible)", "Lock (full)", "Kid mode", "Server…", "", "Quit"]);
        let m = menu(&s);
        assert!(!m[0].2 && !m[2].2 && m[4].2 && m[5].2 && m[6].2 && m[7].2 && m[10].2);
        assert_eq!(tooltip("Stopped"), "Crundi — Stopped");
        assert_eq!(status_label(true), "Running");
        assert_eq!(status_label(false), "Stopped");
        assert!(icon_rgba().is_some_and(|(_, w, h)| w == 32 && h == 32));
    }

    #[test]
    fn kid_mode_locks_it() {
        let s = State { status: "Running".into(), kid_locked: true, wake: None };
        let m = menu(&s);
        let kid = m.iter().find(|x| x.0 == "kid").unwrap();
        assert_eq!((kid.1.as_str(), kid.2), ("Locked", false));
        assert!(!m.iter().find(|x| x.0 == "quit").unwrap().2);
        assert!(!m.iter().find(|x| x.0 == "lock-visible").unwrap().2 && !m.iter().find(|x| x.0 == "lock-full").unwrap().2);
    }

    #[test]
    fn hey_cru() {
        let untrained = State { status: "Running".into(), kid_locked: false, wake: Some((false, false, String::new())) };
        assert_eq!(labels(&untrained), ["Crundi", "", "Running", "", "Open", "Lock (visible)", "Lock (full)", "Kid mode", "Server…", "", WAKE_LABEL, "    Train it first in Settings, Voice assistant", "", "Quit"]);
        let m = menu(&untrained);
        assert_eq!((m[10].2, m[10].3), (false, Some(false)));
        let on = State { wake: Some((true, true, "Listening".into())), ..untrained.clone() };
        let m = menu(&on);
        assert_eq!((m[10].2, m[10].3), (true, Some(true)));
        assert_eq!(m[11].1, "    Listening");
        let off = State { wake: Some((false, true, String::new())), ..untrained };
        assert_eq!(labels(&off), ["Crundi", "", "Running", "", "Open", "Lock (visible)", "Lock (full)", "Kid mode", "Server…", "", WAKE_LABEL, "", "Quit"]);
    }
}
