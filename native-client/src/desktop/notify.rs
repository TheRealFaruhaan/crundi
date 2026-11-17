//! OS notifications the server sends this client (app/main.js
//! showServerNotification; spec specs/desktop/inbox-tray-bubble.md A.5): the
//! "Desktop app" channel's { type: "notify", title, body, url? }. A click
//! brings the window to the front and, for /chat/<id>, opens that chat.
//!
//! Linux: the desktop's notification service over D-Bus (gdbus, with a
//! "default" action; a monitor hears the click). Windows: a toast through
//! PowerShell. macOS: osascript's notification (a click opens nothing there).

use crate::kit;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashMap, process::Command, sync::Mutex};

/// What was shown (for the driver), newest last.
thread_local! {
    static SHOWN: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
}
/// Linux: notification id → the chat to open on its click (kept 6 hours).
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
        let r = if cfg!(target_os = "linux") { linux(&title, &body, &chat) }
            else if cfg!(target_os = "windows") { windows(&title, &body) }
            else { mac(&title, &body) };
        if let Err(e) = r { eprintln!("[notify] could not show: {e}"); }
    });
}

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
            if let Some(chat) = chat { let _ = slint::invoke_from_event_loop(move || clicked(&chat)); }
        }
    });
}

fn windows(title: &str, body: &str) -> Result<(), String> {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\'', "''");
    let script = format!("[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; \
        $x = [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime]::new(); \
        $x.LoadXml('<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>'); \
        [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Crundi').Show([Windows.UI.Notifications.ToastNotification]::new($x))", esc(title), esc(body));
    Command::new("powershell").args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script]).status().map_err(|e| e.to_string()).map(|_| ())
}

fn mac(title: &str, body: &str) -> Result<(), String> {
    let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    Command::new("osascript").args(["-e", &format!("display notification \"{}\" with title \"{}\"", q(body), q(title))]).status().map_err(|e| e.to_string()).map(|_| ())
}

/// A click: the window to the front; a chat's link opens that chat.
pub fn clicked(chat: &str) {
    if let Some(w) = kit::ui() {
        use slint::ComponentHandle;
        let _ = w.show();
        w.window().set_minimized(false);
    }
    if !chat.is_empty() { super::go_to_chat(chat); }
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
    }
}
