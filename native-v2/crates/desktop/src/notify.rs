//! OS notifications the server sends this client (app/main.js
//! showServerNotification; spec specs/desktop/inbox-tray-bubble.md A.5): the
//! "Desktop app" channel's { type: "notify", title, body, url? }. A click
//! brings the window to the front and, for /chat/<id>, opens that chat.
//!
//! Linux: the desktop's notification service over D-Bus (gdbus, with a
//! "default" action; a monitor hears the click). Windows: a toast under the
//! app's AppUserModelID (the installer's shortcut carries it), its click
//! heard in this process (on_activated). macOS: the notification centre
//! under the app's bundle id, waited on for its click on its own thread.
//! Each click goes through the same `clicked` (raise, then the chat).

use crate::ctx as kit;
use serde_json::{json, Value};
use std::{cell::RefCell, sync::Mutex};
#[cfg(target_os = "linux")]
use std::{collections::HashMap, process::Command};

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
fn linux(title: &str, body: &str, chat: &str) -> Result<(), String> {
    let out = Command::new("gdbus").args(["call", "--session", "--dest", "org.freedesktop.Notifications", "--object-path", "/org/freedesktop/Notifications",
        "--method", "org.freedesktop.Notifications.Notify", "Crundi", "0", "", title, body, "['default', 'Open']", "{}", "-1"])
        .output().map_err(|e| e.to_string())?;
    if !out.status.success() { return Err(String::from_utf8_lossy(&out.stderr).trim().to_string()); }
    // "(uint32 7,)"
    let s = String::from_utf8_lossy(&out.stdout);
    let id: u32 = s.split_whitespace().nth(1).and_then(|x| x.trim_end_matches([',', ')']).parse().ok()).unwrap_or(0);
    let mut live = LIVE.lock().unwrap();
    let first = live.is_none();
    let map = live.get_or_insert_with(HashMap::new);
    map.retain(|_, (_, at)| at.elapsed().as_secs() < 6 * 3600);
    map.insert(id, (chat.to_string(), std::time::Instant::now()));
    drop(live);
    if first { monitor(); }
    Ok(())
}

/// Hear the clicks (ActionInvoked) on our notifications.
#[cfg(target_os = "linux")]
fn monitor() {
    std::thread::spawn(|| {
        use std::io::BufRead;
        let Ok(mut child) = Command::new("gdbus").args(["monitor", "--session", "--dest", "org.freedesktop.Notifications"]).stdout(std::process::Stdio::piped()).spawn() else { return };
        let Some(out) = child.stdout.take() else { return };
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            // ".../Notifications: org.freedesktop.Notifications.ActionInvoked (uint32 7, 'default')"
            if !line.contains("ActionInvoked") { continue; }
            let id: Option<u32> = line.split("uint32 ").nth(1).and_then(|x| x.split(',').next()).and_then(|x| x.trim().parse().ok());
            let chat = id.and_then(|id| LIVE.lock().unwrap().as_mut().and_then(|m| m.remove(&id))).map(|x| x.0);
            // The pump takes it (woken from this thread: nothing polls for clicks).
            if let Some(chat) = chat { CLICKS.lock().unwrap().push(chat); crundi_webview::wake(); }
        }
    });
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
}

/// When the area's pump must run next for this part with nothing waking it
/// (lib.rs `next_due`): never. A click wakes the pump from the monitor's
/// thread, and the clicks' map is pruned (6 hours) as a notification is shown.
pub fn next_due() -> Option<std::time::Instant> { None }
