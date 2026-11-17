//! The tray (specs/desktop/inbox-tray-bubble.md A.2-A.3; app/main.js
//! createTray, updateTrayMenu): the head icon, the tooltip "Crundi —
//! <status>", and the menu in the PC client's order and wording, as far as
//! it applies to this client: no Start/Stop (no bundled server: A.3's note),
//! no PC lock or kid mode, no updates (phase 7). A click (Linux; double
//! click elsewhere) shows the window.
//!
//! Linux: a StatusNotifierItem (ksni, over the session D-Bus). Windows and
//! macOS: tray-icon. Either way a pick is queued (its thread, or the menu's
//! channel) and acted on in the area's pump, with the window. No tray (no
//! session bus, no watcher) is logged and the client carries on (A.2).

use crate::ctx;
use serde_json::{json, Value};
use std::sync::Mutex;

/// The head drawn for small sizes (A.2: icon_32x32.png).
const ICON_PNG: &[u8] = include_bytes!("../../../../assets/icon_32x32.png");

/// The menu (A.3): (id, label, enabled); "-" is a separator.
pub fn menu(status: &str) -> Vec<(&'static str, String, bool)> {
    vec![
        ("head", "Crundi".into(), false),
        ("-", String::new(), false),
        ("status", status.to_string(), false),
        ("-", String::new(), false),
        ("open", "Open".into(), true),
        ("kid", "Kid mode".into(), true),
        ("server", "Server\u{2026}".into(), true),
        ("-", String::new(), false),
        ("quit", "Quit".into(), true),
    ]
}

/// A.3's status labels, from the connection: running, or stopped.
pub fn status_label(online: bool) -> &'static str { if online { "Running" } else { "Stopped" } }

/// "Crundi — <status>" (an em dash).
pub fn tooltip(status: &str) -> String { format!("Crundi \u{2014} {status}") }

static PICKS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static SHOWN: Mutex<Option<(bool, String)>> = Mutex::new(None);

fn pick(id: &str) { PICKS.lock().unwrap().push(id.to_string()); }

/// The tray's picks since the last pump (and the platform's events).
pub fn take_picks() -> Vec<String> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    other::poll();
    std::mem::take(&mut *PICKS.lock().unwrap())
}

/// Act on a pick (the area's pump, with the window).
pub fn act(id: &str) {
    match id {
        "open" => ctx::raise(),
        // A.3's "Kid mode": started here (the kid screen opens on this PC).
        "kid" => { ctx::raise(); crate::cru_kid::start("tray"); }
        // The server's settings (the PC client's "Server…" shows its server page).
        "server" => { ctx::raise(); ctx::show_tab("settings"); }
        "quit" => ctx::on_app(|_, cx| cx.quit()),
        _ => {}
    }
}

/// The icon as RGBA, its size.
fn icon_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let i = image::load_from_memory(ICON_PNG).ok()?.to_rgba8();
    let (w, h) = i.dimensions();
    Some((i.into_raw(), w, h))
}

/// Make the tray (once, at start).
pub fn start(online: bool) {
    let status = status_label(online).to_string();
    let ok = platform::start(&status);
    *SHOWN.lock().unwrap() = Some((ok, status));
}

/// The connection changed: the status line and the tooltip follow.
pub fn set_online(online: bool) {
    let status = status_label(online).to_string();
    let mut s = SHOWN.lock().unwrap();
    let Some((ok, cur)) = s.as_mut() else { return };
    if *cur == status { return; }
    *cur = status.clone();
    if *ok { platform::update(&status); }
}

/// What the test driver sees: shown, the tooltip, the menu's labels.
pub fn props() -> Value {
    let s = SHOWN.lock().unwrap().clone();
    let (shown, status) = s.unwrap_or((false, String::new()));
    json!({
        "shown": shown, "tooltip": tooltip(&status),
        "menu": menu(&status).iter().map(|(id, l, en)| json!({ "id": id, "label": l, "enabled": en })).collect::<Vec<_>>(),
    })
}

/// A pick as the tray's menu makes it (the test driver's "tray-pick").
pub fn test_pick(id: &str) { pick(id); }

#[cfg(target_os = "linux")]
mod platform {
    use ksni::blocking::{Handle, TrayMethods};
    use std::sync::Mutex;

    struct Tray { status: String, icon: Vec<ksni::Icon> }

    impl ksni::Tray for Tray {
        fn id(&self) -> String { "crundi".into() }
        fn title(&self) -> String { "Crundi".into() }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> { self.icon.clone() }
        fn tool_tip(&self) -> ksni::ToolTip { ksni::ToolTip { title: super::tooltip(&self.status), ..Default::default() } }
        // Linux: a single click shows the window too (A.2).
        fn activate(&mut self, _x: i32, _y: i32) { super::pick("open"); }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::*;
            super::menu(&self.status).into_iter().map(|(id, label, enabled)| {
                if id == "-" { return MenuItem::Separator; }
                StandardItem { label, enabled, activate: Box::new(move |_: &mut Self| super::pick(id)), ..Default::default() }.into()
            }).collect()
        }
    }

    static HANDLE: Mutex<Option<Handle<Tray>>> = Mutex::new(None);

    pub fn start(status: &str) -> bool {
        // ARGB32, network byte order.
        let icon = super::icon_rgba().map(|(rgba, w, h)| {
            let data = rgba.chunks_exact(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
            vec![ksni::Icon { width: w as i32, height: h as i32, data }]
        }).unwrap_or_default();
        // Registered even before a panel's watcher is there (it finds the item when it comes).
        match (Tray { status: status.to_string(), icon }).assume_sni_available(true).spawn() {
            Ok(h) => { *HANDLE.lock().unwrap() = Some(h); true }
            Err(e) => { eprintln!("[lifecycle] Tray error: {e}"); false }
        }
    }

    pub fn update(status: &str) {
        let s = status.to_string();
        if let Some(h) = HANDLE.lock().unwrap().as_ref() { h.update(move |t| t.status = s); }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
use other as platform;

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod other {
    use std::cell::RefCell;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};

    thread_local! { static TRAY: RefCell<Option<(TrayIcon, Vec<(String, MenuItem)>)>> = const { RefCell::new(None) }; }

    pub fn start(status: &str) -> bool {
        let menu = Menu::new();
        let mut items = vec![];
        for (id, label, enabled) in super::menu(status) {
            if id == "-" { let _ = menu.append(&PredefinedMenuItem::separator()); continue; }
            let it = MenuItem::with_id(id, &label, enabled, None);
            let _ = menu.append(&it);
            items.push((id.to_string(), it));
        }
        let mut b = TrayIconBuilder::new().with_tooltip(super::tooltip(status)).with_menu(Box::new(menu));
        if let Some((rgba, w, h)) = super::icon_rgba() {
            if let Ok(i) = tray_icon::Icon::from_rgba(rgba, w, h) { b = b.with_icon(i); }
        }
        match b.build() {
            Ok(t) => { TRAY.with(|x| *x.borrow_mut() = Some((t, items))); true }
            Err(e) => { eprintln!("[lifecycle] Tray error: {e}"); false }
        }
    }

    pub fn update(status: &str) {
        TRAY.with(|x| if let Some((t, items)) = x.borrow().as_ref() {
            let _ = t.set_tooltip(Some(super::tooltip(status)));
            if let Some((_, it)) = items.iter().find(|(id, _)| id == "status") { it.set_text(status); }
        });
    }

    /// The menu's picks and a double click on the icon (show the window).
    pub fn poll() {
        while let Ok(e) = MenuEvent::receiver().try_recv() { super::pick(&e.id().0); }
        while let Ok(e) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } = e { super::pick("open"); }
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
mod platform {
    pub fn start(_status: &str) -> bool { false }
    pub fn update(_status: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_and_its_words() {
        let m = menu("Running");
        let labels: Vec<&str> = m.iter().map(|x| x.1.as_str()).collect();
        assert_eq!(labels, ["Crundi", "", "Running", "", "Open", "Kid mode", "Server…", "", "Quit"]);
        assert!(!m[0].2 && !m[2].2 && m[4].2);
        assert_eq!(tooltip("Stopped"), "Crundi — Stopped");
        assert_eq!(status_label(true), "Running");
        assert!(icon_rgba().is_some_and(|(_, w, h)| w == 32 && h == 32));
    }
}
