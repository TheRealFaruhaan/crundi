//! A drawer tab's downloads (app/main.js wireDownloads; Electron's own
//! Save dialog for the drawer's partitions): one Save dialog per download,
//! in Downloads with the page's file name filled in; cancelled, nothing is
//! saved. The browser host holds the download until the answer comes
//! (browser-host views.rs `ViewDownloads`). A hidden browser or a chat's
//! frame downloads nothing.

use crate::ctx;
use crate::drawer;
use crundi_webview::engine as web;
use serde_json::{json, Value};
use std::cell::RefCell;

thread_local! {
    /// The downloads asked for this run, newest last (the driver's look).
    static SEEN: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
}

fn note(dl: u32, f: impl FnOnce(&mut serde_json::Map<String, Value>)) {
    SEEN.with(|s| if let Some(Value::Object(m)) = s.borrow_mut().iter_mut().rev().find(|v| v.get("dl").and_then(|d| d.as_u64()) == Some(dl as u64)) { f(m); });
}

/// The Downloads folder (else home): where the dialog opens.
fn downloads_dir() -> std::path::PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(std::path::PathBuf::from).unwrap_or_default();
    if home.join("Downloads").is_dir() { home.join("Downloads") } else { home }
}

/// A page started a download (engine hook): a drawer tab's asks where.
pub fn asked(i: usize, dl: u32, name: &str, url: &str) {
    let Some(tab) = drawer::tab_of_panel(i) else { web::download_to(i, dl, ""); return };
    SEEN.with(|s| s.borrow_mut().push(json!({ "dl": dl, "tab": tab.id, "name": name, "url": url, "state": "asking", "path": "" })));
    // The driver's pick stands in for the dialog.
    if let Some(p) = crundi_core::driver::take_pick() { answer(i, dl, Some(p)); return; }
    let name = name.to_string();
    ctx::on_app(move |_, cx| {
        let rx = cx.prompt_for_new_path(&downloads_dir(), Some(&name));
        cx.spawn(async move |_: &mut gpui::AsyncApp| {
            let p = match rx.await { Ok(Ok(Some(p))) => Some(p), _ => None };
            answer(i, dl, p);
        }).detach();
    });
}

fn answer(i: usize, dl: u32, path: Option<std::path::PathBuf>) {
    let p = path.map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    note(dl, |m| { m.insert("state".into(), json!(if p.is_empty() { "cancelled" } else { "saving" })); m.insert("path".into(), json!(p)); });
    web::download_to(i, dl, &p);
}

/// A download ended (engine hook).
pub fn ended(_i: usize, dl: u32, path: &str, ok: bool) {
    note(dl, |m| { m.insert("state".into(), json!(if ok { "done" } else { "failed" })); if !path.is_empty() { m.insert("path".into(), json!(path)); } });
    if ok { eprintln!("[browser] download saved: {path}"); }
}

pub fn props() -> Value { SEEN.with(|s| Value::Array(s.borrow().clone())) }
