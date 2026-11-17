//! The Inbox (src/webapp-html.js ibx*; spec specs/desktop/inbox-tray-bubble.md
//! B): everything meant for the owner, by thread. The sidebar's Inbox opens
//! it as a page (tab id "inbox", no tab button); the live "inbox" event keeps
//! it and the sidebar's count current. An older server (404) or a
//! collaborator (403) has none: the Inbox button then stays quiet.

//!
//! Ported from the Slint client's src/desktop/inbox.rs; the page is
//! inbox_ui.rs, drawn from this state.

use crate::ctx as kit;
use crundi_webview::later;
use gpui::RenderImage;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashMap, sync::Arc, time::Duration};

pub const PAGE: usize = 100;

#[derive(Default)]
pub struct Inbox {
    pub supported: bool,
    pub threads: Vec<Value>,
    pub state: String,
    pub open: String,
    pub items: Vec<Value>,
    pub more: bool,
    /// Items unread when the thread was opened (their accent edge).
    pub fresh: std::collections::HashSet<String>,
    /// Pictures by their url (None: could not be read).
    pub pics: HashMap<String, Option<Arc<RenderImage>>>,
    pub unread: i64,
    reload_gen: u64,
    /// The open thread's messages moved: the page scrolls to the newest.
    pub pin_gen: u64,
}

thread_local! { static I: RefCell<Inbox> = RefCell::new(Inbox { supported: true, state: "loading".into(), ..Default::default() }); }
pub fn with<R>(f: impl FnOnce(&mut Inbox) -> R) -> R { I.with(|i| f(&mut i.borrow_mut())) }

pub fn now_ms() -> f64 { crundi_webview::engine::now_ms() as f64 }

/// The list's when (ibxWhen): now, Nm, Nh, a weekday, "Nov 3".
pub fn when(now: f64, at: f64) -> String {
    let s = ((now - at) / 1000.0).max(0.0);
    if s < 60.0 { return "now".into(); }
    if s < 3600.0 { return format!("{}m", (s / 60.0) as i64); }
    if s < 86400.0 { return format!("{}h", (s / 3600.0) as i64); }
    let Some(d) = chrono::DateTime::from_timestamp_millis(at as i64).map(|d| d.with_timezone(&chrono::Local)) else { return String::new() };
    if s < 6.0 * 86400.0 { return d.format("%a").to_string(); }
    d.format("%b %-d").to_string()
}

/// A message's time in full: "Nov 3, 14:05".
pub fn when_full(at: f64) -> String {
    chrono::DateTime::from_timestamp_millis(at as i64).map(|d| d.with_timezone(&chrono::Local).format("%b %-d, %H:%M").to_string()).unwrap_or_default()
}

/// "N B", "N KB", "N.N MB".
pub fn size(b: f64) -> String {
    if b < 1024.0 { format!("{} B", b as i64) } else if b < 1024.0 * 1024.0 { format!("{} KB", (b / 1024.0).round() as i64) } else { format!("{:.1} MB", b / 1024.0 / 1024.0) }
}

pub fn badge(n: i64) -> String { if n > 99 { "99+".into() } else { n.to_string() } }

/// A message kind's name (IBX_KIND).
pub fn kind_label(k: &str) -> &'static str {
    match k { "photo" => "Picture", "file" => "File", "alert" => "Alert", "question" => "Question", "permission" => "Permission", "report" => "Report", "schedule" => "Scheduled job", "system" => "Crundi", _ => "Message" }
}

pub fn is_pic(mime: &str) -> bool { matches!(mime, "image/png" | "image/jpeg" | "image/gif" | "image/webp") }

// ─── Loading ───

/// The count for the sidebar (on every connect) and the list when shown.
pub fn load() {
    kit::call(|api| api.get("/api/inbox/threads"), |r| match r {
        Ok(v) => {
            with(|i| { i.supported = true; i.threads = v.get("threads").and_then(|t| t.as_array()).cloned().unwrap_or_default(); i.state = "ok".into(); i.unread = kit::n(&v, "unread") as i64; });
            set_unread(kit::n(&v, "unread") as i64);
            render();
        }
        Err(e) => {
            // An older server or a collaborator: no Inbox, said nowhere.
            let gone = e.contains("Not supported") || e.contains("403") || e.to_lowercase().contains("forbidden");
            with(|i| { if gone { i.supported = false; } i.state = if gone { "ok".into() } else { "error".into() }; });
            render();
        }
    });
}

fn set_unread(n: i64) { with(|i| i.unread = n); }

pub fn shown() -> bool { kit::get().tab == "inbox" }

/// The sidebar's Inbox.
pub fn open_page() {
    kit::show_tab("inbox");
    load();
    let open = with(|i| i.open.clone());
    if !open.is_empty() { load_thread(&open, false); }
}

pub fn open_thread(key: &str) {
    let unread = with(|i| {
        i.open = key.to_string(); i.items.clear(); i.more = false; i.fresh.clear();
        i.threads.iter().find(|t| kit::s(t, "key") == key).map(|t| kit::n(t, "unread") as i64).unwrap_or(0)
    });
    render();
    load_thread(key, true);
    if unread > 0 { post_read(json!({ "thread": key })); }
    with(|i| if let Some(t) = i.threads.iter_mut().find(|t| kit::s(t, "key") == key) { t["unread"] = json!(0); });
}

fn load_thread(key: &str, first: bool) {
    let k = key.to_string();
    kit::call(move |api| api.get(&format!("/api/inbox?thread={}&limit={PAGE}", kit::enc(&k))), {
        let key = key.to_string();
        move |r| match r {
            Ok(v) => {
                let items = v.get("items").and_then(|t| t.as_array()).cloned().unwrap_or_default();
                with(|i| {
                    if i.open != key { return; }
                    if first { i.fresh = items.iter().filter(|x| !x.get("read").and_then(|r| r.as_bool()).unwrap_or(false)).map(|x| kit::s(x, "id")).collect(); }
                    i.more = items.len() >= PAGE;
                    i.items = items;
                });
                fetch_pics();
                with(|i| i.pin_gen += 1);
                render();
            }
            Err(e) => kit::toast_error(&e),
        }
    });
}

/// Show older (before the oldest one shown).
pub fn older() {
    let (key, before) = with(|i| (i.open.clone(), i.items.last().map(|x| kit::s(x, "id")).unwrap_or_default()));
    if key.is_empty() || before.is_empty() { return; }
    kit::call(move |api| api.get(&format!("/api/inbox?thread={}&limit={PAGE}&before={}", kit::enc(&key), kit::enc(&before))), |r| match r {
        Ok(v) => {
            let items = v.get("items").and_then(|t| t.as_array()).cloned().unwrap_or_default();
            with(|i| { i.more = items.len() >= PAGE; i.items.extend(items); });
            fetch_pics();
            render();
        }
        Err(e) => kit::toast_error(&e),
    });
}

fn fetch_pics() {
    let want: Vec<String> = with(|i| i.items.iter().flat_map(|x| x.get("media").and_then(|m| m.as_array()).cloned().unwrap_or_default())
        .filter(|m| is_pic(&kit::s(m, "mime")) && !kit::s(m, "url").is_empty()).map(|m| kit::s(&m, "url")).filter(|u| !i.pics.contains_key(u)).collect());
    for url in want {
        let u = url.clone();
        kit::bg(move |api| api.get_bytes(&u).ok().filter(|(s, _)| *s == 200).map(|(_, b)| b), move |b| {
            let img = b.and_then(|b| image::load_from_memory(&b).ok()).map(|i| {
                // GPUI's images are BGRA.
                let mut i = i.thumbnail(1600, 1600).to_rgba8();
                for px in i.chunks_exact_mut(4) { px.swap(0, 2); }
                Arc::new(RenderImage::new([image::Frame::new(i)]))
            });
            with(|i| { i.pics.insert(url, img); });
            render();
        });
    }
}

fn post_read(body: Value) {
    kit::call(move |api| api.post("/api/inbox/read", &body), |r| match r {
        Ok(v) => { if v.get("unread").is_some() { set_unread(kit::n(&v, "unread") as i64); } load(); }
        Err(e) => kit::toast_error(&format!("Could not mark it read: {e}")),
    });
}

pub fn mark_all() {
    set_unread(0);
    with(|i| for t in i.threads.iter_mut() { t["unread"] = json!(0); });
    render();
    kit::call(|api| api.post("/api/inbox/read", &json!({ "all": true })), |r| match r {
        Ok(v) => { set_unread(kit::n(&v, "unread") as i64); load(); }
        Err(e) => kit::toast_error(&format!("Could not mark them read: {e}")),
    });
}

pub fn mark_read() {
    let key = with(|i| i.open.clone());
    if key.is_empty() { return; }
    post_read(json!({ "thread": key }));
}

pub fn delete_thread() {
    let key = with(|i| i.open.clone());
    if key.is_empty() { return; }
    kit::confirm("Delete this whole conversation from the inbox?", "Delete", true, move |ok| {
        if !ok { return; }
        let key = key.clone();
        kit::call(move |api| api.post("/api/inbox/delete", &json!({ "thread": key })), |r| match r {
            Ok(v) => { with(|i| { i.open.clear(); i.items.clear(); }); set_unread(kit::n(&v, "unread") as i64); load(); render(); }
            Err(e) => kit::toast_error(&format!("Could not delete: {e}")),
        });
    });
}

pub fn delete_item(id: &str) {
    let id = id.to_string();
    let id2 = id.clone();
    kit::call(move |api| api.post("/api/inbox/delete", &json!({ "ids": [id] })), move |r| match r {
        Ok(v) => {
            let empty = with(|i| { i.items.retain(|x| kit::s(x, "id") != id2); if i.items.is_empty() { i.open.clear(); } i.items.is_empty() });
            let _ = empty;
            set_unread(kit::n(&v, "unread") as i64);
            load();
            render();
        }
        Err(e) => kit::toast_error(&format!("Could not delete: {e}")),
    });
}

/// Open chat: the chat's pane, or its project with a word when it is gone.
pub fn open_chat() {
    let t = with(|i| i.threads.iter().find(|t| kit::s(t, "key") == i.open).cloned());
    let Some(t) = t else { return };
    let chat = kit::s(&t, "chatId");
    let live = kit::get().terminals.iter().any(|x| kit::s(x, "id") == chat && kit::s(x, "status") != "exited");
    if live { crate::go_to_chat(&chat); } else {
        let p = kit::s(&t, "project");
        if !p.is_empty() { kit::select_project(&p); }
        kit::show_tab("workbench");
        kit::toast_error("That chat is not open any more");
    }
}

/// A picture opens outside (the system's viewer), a file downloads.
pub fn media(url: &str, pic: bool) {
    let path = if pic { url.to_string() } else { format!("{url}{}download=1", if url.contains('?') { '&' } else { '?' }) };
    let name = url.rsplit('/').next().unwrap_or("file").split('?').next().unwrap_or("file").to_string();
    kit::bg(move |api| api.get_bytes(&path).ok().filter(|(s, _)| *s == 200).map(|(_, b)| b), move |b| {
        let Some(b) = b else { return kit::toast_error("Download failed") };
        let dir = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(|h| std::path::PathBuf::from(h).join("Downloads")).unwrap_or_else(std::env::temp_dir);
        let _ = std::fs::create_dir_all(&dir);
        let mut dest = dir.join(&name);
        let mut n = 1;
        while dest.exists() { dest = dir.join(format!("{n}-{name}")); n += 1; }
        match std::fs::write(&dest, b) {
            Ok(()) => { if pic { let _ = open::that_detached(&dest); } else { kit::toast_ok(&format!("Saved to {}", dest.display())); } }
            Err(e) => kit::toast_error(&format!("Download failed: {e}")),
        }
    });
}

/// The live "inbox" event: the count; the open thread again, read when shown
/// (debounced 150 ms, ibxOnEvent).
pub fn event(v: &Value) {
    if v.get("unread").is_some() { set_unread(kit::n(v, "unread") as i64); }
    let reload = with(|i| { i.reload_gen += 1; i.reload_gen });
    let added = v.get("item").is_some();
    let thread = v.get("thread").and_then(|t| t.as_str()).map(str::to_string)
        .or_else(|| v.get("item").map(|x| kit::s(x, "thread")));
    later(Duration::from_millis(150), move || {
        if with(|i| i.reload_gen != reload) { return; }
        let open = with(|i| i.open.clone());
        if shown() && !open.is_empty() && thread.as_deref().is_none_or(|t| t == open) {
            load_thread(&open, false);
            if added { post_read(json!({ "thread": open })); return; }
        }
        load();
    });
}

// ─── Drawing ───

/// The page draws again (inbox_ui.rs reads the state).
pub fn render() { kit::dirty(); }

/// The header's second line: "N unread", "All read", or nothing.
pub fn sub() -> String {
    with(|i| if i.unread > 0 { format!("{} unread", i.unread) } else if !i.threads.is_empty() { "All read".into() } else { String::new() })
}

pub fn signed_out() {
    with(|i| { *i = Inbox { supported: true, state: "loading".into(), ..Default::default() }; });
    render();
}

pub fn props() -> Value {
    with(|i| json!({
        "supported": i.supported, "state": i.state, "unread": i.unread, "open": i.open, "more": i.more,
        "threads": i.threads.iter().map(|t| json!({ "key": t["key"], "title": t["title"], "unread": t["unread"] })).collect::<Vec<_>>(),
        "items": i.items.iter().map(|x| json!({ "id": x["id"], "title": x["title"], "kind": x["kind"], "media": x.get("media").and_then(|m| m.as_array()).map(|m| m.len()).unwrap_or(0) })).collect::<Vec<_>>(),
        "pics": i.pics.len(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words() {
        assert_eq!(when(100_000.0, 90_000.0), "now");
        assert_eq!(when(10_000_000.0, 10_000_000.0 - 5.0 * 60_000.0), "5m");
        assert_eq!(when(1e9, 1e9 - 3.0 * 3_600_000.0), "3h");
        assert_eq!(size(500.0), "500 B");
        assert_eq!(size(2048.0), "2 KB");
        assert_eq!(size(3.5 * 1024.0 * 1024.0), "3.5 MB");
        assert_eq!(badge(120), "99+");
        assert_eq!(kind_label("schedule"), "Scheduled job");
        assert_eq!(kind_label("weird"), "Message");
    }
}
