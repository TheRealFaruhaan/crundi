//! The Inbox (src/webapp-html.js ibx*; spec specs/desktop/inbox-tray-bubble.md
//! B): everything meant for the owner, by thread. The sidebar's Inbox opens
//! it as a page (tab id "inbox", no tab button); the live "inbox" event keeps
//! it and the sidebar's count current. An older server (404) or a
//! collaborator (403) has none: the Inbox button then stays quiet.

use crate::{kit, DesktopInbox, DesktopItem, DesktopMedia, DesktopThread};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, VecModel};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

pub const PAGE: usize = 100;

#[derive(Default)]
struct Inbox {
    supported: bool,
    threads: Vec<Value>,
    state: String,
    open: String,
    items: Vec<Value>,
    more: bool,
    /// Items unread when the thread was opened (their accent edge).
    fresh: std::collections::HashSet<String>,
    pics: HashMap<String, slint::Image>,
    unread: i64,
    reload_gen: u64,
}

thread_local! { static I: RefCell<Inbox> = RefCell::new(Inbox { supported: true, state: "loading".into(), ..Default::default() }); }
fn with<R>(f: impl FnOnce(&mut Inbox) -> R) -> R { I.with(|i| f(&mut i.borrow_mut())) }

fn now_ms() -> f64 { crate::browser::now_ms() as f64 }

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

fn is_pic(mime: &str) -> bool { matches!(mime, "image/png" | "image/jpeg" | "image/gif" | "image/webp") }

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

fn set_unread(n: i64) {
    with(|i| i.unread = n);
    if let Some(w) = kit::ui() { w.set_inbox_unread(n as i32); w.global::<DesktopInbox>().set_unread(n as i32); }
}

pub fn shown() -> bool { kit::ui().map(|w| w.get_current_tab() == "inbox").unwrap_or(false) }

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
                render();
                if let Some(w) = kit::ui() { let g = w.global::<DesktopInbox>(); g.set_pin_gen(g.get_pin_gen() + 1); }
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
        let Some(api) = kit::api() else { return };
        let u = url.clone();
        kit::bg(move || api.get_bytes(&u).ok().filter(|(s, _)| *s == 200).map(|(_, b)| b), move |b| {
            let img = b.and_then(|b| image::load_from_memory(&b).ok()).map(|i| {
                let i = i.thumbnail(1600, 1600).to_rgba8();
                slint::Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(i.as_raw(), i.width(), i.height()))
            });
            with(|i| { i.pics.insert(url, img.unwrap_or_default()); });
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
    kit::confirm(kit::Confirm { title: "Delete this whole conversation from the inbox?", message: "", ok: "Delete", danger: true }, move |ok| {
        if !ok { return; }
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
    let live = kit::terminals().unwrap_or_default().iter().any(|x| kit::s(x, "id") == chat && kit::s(x, "status") != "exited");
    if live { super::go_to_chat(&chat); } else {
        let p = kit::s(&t, "project");
        if !p.is_empty() { crate::app::select_project(&p); }
        kit::show_tab("workbench");
        kit::toast_error("That chat is not open any more");
    }
}

/// A picture opens outside (the system's viewer), a file downloads.
pub fn media(url: &str, pic: bool) {
    let path = if pic { url.to_string() } else { format!("{url}{}download=1", if url.contains('?') { '&' } else { '?' }) };
    let name = url.rsplit('/').next().unwrap_or("file").split('?').next().unwrap_or("file").to_string();
    let Some(api) = kit::api() else { return };
    kit::bg(move || api.get_bytes(&path).ok().filter(|(s, _)| *s == 200).map(|(_, b)| b), move |b| {
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
    let gen = with(|i| { i.reload_gen += 1; i.reload_gen });
    let added = v.get("item").is_some();
    let thread = v.get("thread").and_then(|t| t.as_str()).map(str::to_string)
        .or_else(|| v.get("item").map(|x| kit::s(x, "thread")));
    slint::Timer::single_shot(Duration::from_millis(150), move || {
        if with(|i| i.reload_gen != gen) { return; }
        let open = with(|i| i.open.clone());
        if shown() && !open.is_empty() && thread.as_deref().is_none_or(|t| t == open) {
            load_thread(&open, false);
            if added { post_read(json!({ "thread": open })); return; }
        }
        load();
    });
}

// ─── Drawing ───

pub fn render() {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<DesktopInbox>();
    let (threads, state, open, items, more, fresh, pics, unread) = with(|i| (i.threads.clone(), i.state.clone(), i.open.clone(), i.items.clone(), i.more, i.fresh.clone(), i.pics.clone(), i.unread));
    let now = now_ms();
    let rows: Vec<DesktopThread> = threads.iter().map(|t| {
        let last = t.get("last").cloned().unwrap_or(Value::Null);
        let u = kit::n(t, "unread") as i64;
        DesktopThread {
            key: kit::s(t, "key").into(), kind: kit::s(t, "kind").into(), title: kit::s(t, "title").into(), sub: kit::s(t, "subtitle").into(),
            last: Some(kit::s(&last, "title")).filter(|x| !x.is_empty()).unwrap_or_else(|| kit::s(&last, "text")).into(),
            unread: u as i32, badge: badge(u).into(), age: when(now, kit::n(&last, "at")).into(), selected: kit::s(t, "key") == open,
        }
    }).collect();
    sync(&g.get_threads(), rows, |m| g.set_threads(m));
    g.set_list_state(state.into());
    g.set_sub(if unread > 0 { format!("{unread} unread") } else if !threads.is_empty() { "All read".into() } else { String::new() }.into());
    let t = threads.iter().find(|t| kit::s(t, "key") == open);
    g.set_open_key(if t.is_some() { open.clone().into() } else { "".into() });
    if let Some(t) = t {
        g.set_open_kind(kit::s(t, "kind").into());
        g.set_open_title(kit::s(t, "title").into());
        g.set_open_sub(kit::s(t, "subtitle").into());
        g.set_open_chat(kit::s(t, "kind") == "chat" && !kit::s(t, "chatId").is_empty());
    }
    g.set_more(more);
    // Oldest first.
    let rows: Vec<DesktopItem> = items.iter().rev().map(|x| {
        let media: Vec<DesktopMedia> = x.get("media").and_then(|m| m.as_array()).cloned().unwrap_or_default().iter().map(|m| {
            let url = kit::s(m, "url");
            let pic = pics.get(&url).cloned();
            DesktopMedia { name: kit::s(m, "name").into(), size: size(kit::n(m, "size")).into(), url: url.clone().into(), is_pic: is_pic(&kit::s(m, "mime")), kept: !url.is_empty(), loaded: pic.is_some(), pic: pic.unwrap_or_default() }
        }).collect();
        let kind = kit::s(x, "kind");
        DesktopItem {
            id: kit::s(x, "id").into(), kind_label: kind_label(&kind).into(), kind: kind.into(),
            title: kit::s(x, "title").into(), text: kit::s(x, "text").into(), age: when_full(kit::n(x, "at")).into(),
            fresh: fresh.contains(&kit::s(x, "id")), media: Rc::new(VecModel::from(media)).into(),
        }
    }).collect();
    g.set_items(Rc::new(VecModel::from(rows)).into());
}

fn sync<T: Clone + PartialEq + 'static>(m: &slint::ModelRc<T>, rows: Vec<T>, set: impl FnOnce(slint::ModelRc<T>)) {
    match m.as_any().downcast_ref::<VecModel<T>>() { Some(vm) => kit::sync_rows(vm, rows), None => set(Rc::new(VecModel::from(rows)).into()) }
}

pub fn init(w: &crate::MainWindow) {
    let g = w.global::<DesktopInbox>();
    g.on_open(|k| open_thread(&k));
    g.on_mark_all(mark_all);
    g.on_mark_read(mark_read);
    g.on_open_chat_clicked(open_chat);
    g.on_delete_thread(delete_thread);
    g.on_delete_item(|id| delete_item(&id));
    g.on_older(older);
    g.on_media(|u, p| media(&u, p));
    g.on_retry(load);
    w.global::<crate::Shell>().on_inbox(open_page);
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
