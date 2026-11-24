//! OS notifications the server sends this client (app/main.js
//! showServerNotification; spec specs/desktop/inbox-tray-bubble.md A.5): the
//! "Desktop app" channel's { type: "notify", title, body, url? }. A click
//! brings the window to the front and, for /chat/<id>, opens that chat.
//!
//! Linux: the desktop's notification service over D-Bus (zbus, with a
//! "default" action; its ActionInvoked signal is the click). Windows: a toast under the
//! app's AppUserModelID (the installer's shortcut carries it), its click
//! heard in this process (on_activated). macOS: the notification centre
//! under the app's bundle id, waited on for its click on its own thread.
//! Each click goes through the same `clicked` (raise, then the chat).

use crate::ctx as kit;
use serde_json::{json, Value};
use std::{cell::RefCell, sync::Mutex};
#[cfg(target_os = "linux")]
use std::collections::HashMap;

// What was shown (for the driver), newest last.
thread_local! {
    static SHOWN: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
}
/// Linux: notification id → the chat to open on its click (kept 6 hours).
#[cfg(target_os = "linux")]
static LIVE: Mutex<Option<HashMap<u32, (String, std::time::Instant)>>> = Mutex::new(None);

/// A chat link's id, re-checked (^/chat/[A-Za-z0-9_-]{1,80}$).
pub fn chat_of_url(url: &str) -> Option<String> {
    let id = url.strip_prefix("/chat/")?;
    ((1..=80).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')).then(|| id.to_string())
}

fn clip(s: &str, n: usize) -> String { s.chars().take(n).collect() }

/// The server's notice.
pub fn from_server(m: &Value) {
    let title = clip(&Some(kit::s(m, "title")).filter(|t| !t.is_empty()).unwrap_or_else(|| "Crundi".into()), 120);
    let body = clip(&kit::s(m, "body"), 400);
    let chat = chat_of_url(&kit::s(m, "url")).unwrap_or_default();
    SHOWN.with(|s| s.borrow_mut().push(json!({ "title": title, "body": body, "chat": chat })));
    show(title, body, chat);
}

/// The client's own notice (the worker's job and chat lines), no chat to open.
pub fn show_plain(title: &str, body: &str) {
    SHOWN.with(|s| s.borrow_mut().push(json!({ "title": title, "body": body, "chat": "" })));
    show(title.to_string(), body.to_string(), String::new());
}

fn show(title: String, body: String, chat: String) {
    std::thread::spawn(move || {
        #[cfg(target_os = "linux")]
        let r = linux(&title, &body, &chat);
        #[cfg(target_os = "windows")]
        let r = windows(&title, &body, chat);
        #[cfg(target_os = "macos")]
        let r = mac(&title, &body, chat);
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        let r: Result<(), String> = { let _ = (&title, &body, chat); Ok(()) };
        if let Err(e) = r { eprintln!("[notify] could not show: {e}"); }
    });
}

#[cfg(target_os = "linux")]
const DEST: &str = "org.freedesktop.Notifications";
#[cfg(target_os = "linux")]
const PATH: &str = "/org/freedesktop/Notifications";

/// The session bus, opened once; its click listener starts with it.
#[cfg(target_os = "linux")]
fn bus() -> Result<zbus::blocking::Connection, String> {
    static BUS: Mutex<Option<zbus::blocking::Connection>> = Mutex::new(None);
    let mut b = BUS.lock().unwrap();
    if let Some(c) = b.as_ref() { return Ok(c.clone()); }
    let c = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
    listen(c.clone())?;
    *b = Some(c.clone());
    Ok(c)
}

#[cfg(target_os = "linux")]
fn linux(title: &str, body: &str, chat: &str) -> Result<(), String> { notify_on(&bus()?, title, body, chat).map(|_| ()) }

/// Notify(app_name, replaces_id, app_icon, summary, body, actions, hints,
/// expire_timeout) as the spec has it (FINDINGS T9: gdbus took "-1" for an
/// option and printed its usage); a "default" action, so a click is heard.
#[cfg(target_os = "linux")]
fn notify_on(c: &zbus::blocking::Connection, title: &str, body: &str, chat: &str) -> Result<u32, String> {
    use zbus::zvariant::Value as V;
    let entry = crundi_shell::lifecycle::window_class(true);
    let hints: HashMap<&str, V> = HashMap::from([("desktop-entry", V::from(entry))]);
    let actions = vec!["default", "Open"];
    let reply = c.call_method(Some(DEST), PATH, Some(DEST), "Notify",
        &(crundi_shell::lifecycle::app_name(), 0u32, entry, title, body, actions, hints, -1i32))
        .map_err(|e| e.to_string())?;
    let id: u32 = reply.body().deserialize().map_err(|e| e.to_string())?;
    let mut live = LIVE.lock().unwrap();
    let map = live.get_or_insert_with(HashMap::new);
    map.retain(|_, (_, at)| at.elapsed().as_secs() < 6 * 3600);
    map.insert(id, (chat.to_string(), std::time::Instant::now()));
    Ok(id)
}

/// Hear the clicks (ActionInvoked) on our notifications, on its own thread.
#[cfg(target_os = "linux")]
fn listen(c: zbus::blocking::Connection) -> Result<(), String> {
    let proxy = zbus::blocking::Proxy::new(&c, DEST, PATH, DEST).map_err(|e| e.to_string())?;
    let sigs = proxy.receive_signal("ActionInvoked").map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let _proxy = &proxy;
        for m in sigs {
            let Ok((id, _action)) = m.body().deserialize::<(u32, String)>() else { continue };
            let chat = LIVE.lock().unwrap().as_mut().and_then(|m| m.remove(&id)).map(|x| x.0);
            // The pump takes it (woken from this thread: nothing polls for clicks).
            if let Some(chat) = chat { CLICKS.lock().unwrap().push(chat); crundi_webview::wake(); }
        }
    });
    Ok(())
}

/// The app's id with the OS (contract: com.crundi.app, the client edition
/// com.crundi.client): the all-in-one has its server beside it.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn app_id() -> &'static str {
    let all_in_one = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("server").join("src").join("index.js").is_file())).unwrap_or(false);
    if all_in_one { "com.crundi.app" } else { "com.crundi.client" }
}

/// A click heard on any thread: the pump takes it.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn heard(chat: String) { CLICKS.lock().unwrap().push(chat); crundi_webview::wake(); }

#[cfg(target_os = "windows")]
fn windows(title: &str, body: &str, chat: String) -> Result<(), String> {
    tauri_winrt_notification::Toast::new(app_id()).title(title).text1(body)
        .on_activated(move |_| { heard(chat.clone()); Ok(()) })
        .show().map_err(|e| e.to_string())
}

/// Shown and waited on (this thread is the notification's own); a click on it
/// or its Open button opens it. Kept at most 6 hours, as main.js keeps them.
#[cfg(target_os = "macos")]
fn mac(title: &str, body: &str, chat: String) -> Result<(), String> {
    use mac_notification_sys::{Notification, NotificationResponse};
    static APP: std::sync::Once = std::sync::Once::new();
    APP.call_once(|| { let _ = mac_notification_sys::set_application(app_id()); });
    let mut n = Notification::new();
    n.title(title).message(body).wait_for_click(true);
    match n.send().map_err(|e| e.to_string())? {
        NotificationResponse::Click | NotificationResponse::ActionButton(_) => heard(chat),
        _ => {}
    }
    Ok(())
}

/// Clicks heard on the monitor's thread, for the main thread (`pump`).
static CLICKS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The clicks heard since the last pump.
pub fn pump() { let all = std::mem::take(&mut *CLICKS.lock().unwrap()); for c in all { clicked(&c); } }

/// A click: the window to the front; a chat's link opens that chat.
pub fn clicked(chat: &str) {
    kit::raise();
    if !chat.is_empty() { crate::go_to_chat(chat); }
}

pub fn props() -> Value { SHOWN.with(|s| Value::Array(s.borrow().clone())) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_links() {
        assert_eq!(chat_of_url("/chat/abc_1-2").as_deref(), Some("abc_1-2"));
        assert_eq!(chat_of_url("/chat/"), None);
        assert_eq!(chat_of_url("/chat/a/b"), None);
        assert_eq!(chat_of_url("https://x/chat/a"), None);
        assert_eq!(chat_of_url(&format!("/chat/{}", "a".repeat(80))).map(|s| s.len()), Some(80));
        assert_eq!(chat_of_url(&format!("/chat/{}", "a".repeat(81))), None);
        assert_eq!(chat_of_url("/chat/a b"), None);
        assert_eq!(clip("héllo", 2), "hé");
    }

    /// FINDINGS T9: against a real bus (a private dbus-daemon) with a
    /// notification service on it: Notify's arguments as the spec has them,
    /// then an ActionInvoked click comes back as the chat to open.
    #[cfg(target_os = "linux")]
    #[test]
    fn notifies_over_dbus_and_hears_the_click() {
        use std::{io::BufRead, sync::Arc};
        use zbus::zvariant::OwnedValue;
        let Ok(mut daemon) = std::process::Command::new("dbus-daemon").args(["--session", "--nofork", "--print-address=1"])
            .stdout(std::process::Stdio::piped()).spawn() else { eprintln!("no dbus-daemon: skipped"); return };
        let mut addr = String::new();
        std::io::BufReader::new(daemon.stdout.take().unwrap()).read_line(&mut addr).unwrap();
        type Seen = Arc<Mutex<Vec<(String, u32, String, String, String, Vec<String>, HashMap<String, OwnedValue>, i32)>>>;
        struct Daemon(Seen);
        #[zbus::interface(name = "org.freedesktop.Notifications")]
        impl Daemon {
            #[allow(clippy::too_many_arguments)]
            fn notify(&self, app: String, replaces: u32, icon: String, summary: String, body: String, actions: Vec<String>, hints: HashMap<String, OwnedValue>, expire: i32) -> u32 {
                let mut s = self.0.lock().unwrap();
                s.push((app, replaces, icon, summary, body, actions, hints, expire));
                41 + s.len() as u32
            }
        }
        let seen: Seen = Arc::default();
        let server = zbus::blocking::connection::Builder::address(addr.trim()).unwrap()
            .name(DEST).unwrap().serve_at(PATH, Daemon(seen.clone())).unwrap().build().unwrap();
        let client = zbus::blocking::connection::Builder::address(addr.trim()).unwrap().build().unwrap();
        listen(client.clone()).unwrap();

        // A title and body an argument parser would choke on.
        let id = notify_on(&client, "-1 Done", "--help 'quoted' {}", "chat_42").unwrap();
        assert_eq!(id, 42);
        {
            let s = seen.lock().unwrap();
            let (app, replaces, icon, summary, body, actions, hints, expire) = &s[0];
            assert_eq!(app, crundi_shell::lifecycle::app_name());
            assert_eq!(*replaces, 0);
            assert_eq!(icon, crundi_shell::lifecycle::window_class(true));
            assert_eq!((summary.as_str(), body.as_str()), ("-1 Done", "--help 'quoted' {}"));
            assert_eq!(actions, &["default", "Open"]);
            assert_eq!(String::try_from(hints["desktop-entry"].try_clone().unwrap()).unwrap(), crundi_shell::lifecycle::window_class(true));
            assert_eq!(*expire, -1);
        }
        // Someone else's notification's click is not ours.
        server.emit_signal(None::<()>, PATH, DEST, "ActionInvoked", &(7u32, "default")).unwrap();
        server.emit_signal(None::<()>, PATH, DEST, "ActionInvoked", &(42u32, "default")).unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while CLICKS.lock().unwrap().is_empty() && std::time::Instant::now() < until { std::thread::sleep(std::time::Duration::from_millis(20)); }
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(*CLICKS.lock().unwrap(), vec!["chat_42".to_string()]);
        let _ = daemon.kill();
        let _ = daemon.wait();
    }
}

/// When the area's pump must run next for this part with nothing waking it
/// (lib.rs `next_due`): never. A click wakes the pump from the monitor's
/// thread, and the clicks' map is pruned (6 hours) as a notification is shown.
pub fn next_due() -> Option<std::time::Instant> { None }
