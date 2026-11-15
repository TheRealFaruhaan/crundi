//! Media (the web's loadMedia, renderMediaBrowser and the media* actions):
//! the library of this project (or every project), filtered by where an
//! item hangs (All, Kanban, Mindmap, Unlinked) and by the search box; each
//! card's View (the lightbox: a picture, or the system's player for video,
//! audio and PDF), Get, Copy path, Jump to its card or idea, Delete; Upload.

use super::g;
use crate::{kit, MainWindow, WorkspaceMediaCard};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

struct St {
    kind: String,
    scope: String,
    q: String,
    items: Vec<Value>,
    state: String,
    thumbs: HashMap<String, Option<slint::Image>>,
    cards: Rc<VecModel<WorkspaceMediaCard>>,
    lb: Option<Value>,
    gen: u64,
}

thread_local! {
    static ST: RefCell<St> = RefCell::new(St { kind: "all".into(), scope: "project".into(), q: String::new(), items: vec![], state: "loading".into(), thumbs: HashMap::new(), cards: Rc::new(VecModel::default()), lb: None, gen: 0 });
}

fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { ST.with(|s| f(&mut s.borrow_mut())) }

/// The list's address (mediaListUrl): this project and the general ones,
/// or all; a kind filter.
pub fn list_url(scope: &str, kind: &str, project: &str) -> String {
    let mut p: Vec<String> = vec![];
    if scope == "project" && !project.is_empty() { p.push(format!("project={}", kit::enc(project))); p.push("includeGeneral=1".into()); }
    if kind != "all" && !kind.is_empty() { p.push(format!("kind={kind}")); }
    format!("/api/media/list?{}", p.join("&"))
}

/// The search (mediaFilter): name, link label, project, extension, kind.
pub fn matches(it: &Value, q: &str) -> bool {
    let q = q.trim().to_lowercase();
    if q.is_empty() { return true; }
    ["originalName", "linkLabel", "projectName", "ext", "kind"].iter().any(|k| kit::s(it, k).to_lowercase().contains(&q))
}

/// mtrunc: at most n characters, the last an ellipsis.
pub fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() > n { s.chars().take(n - 1).collect::<String>() + "\u{2026}" } else { s.to_string() }
}

/// A card's link line (mediaLinkSubHtml): Idea / Subtask / Task.
pub fn link_word(it: &Value) -> String {
    match it.get("link").map(|l| kit::s(l, "type")) {
        None => String::new(),
        Some(t) if t == "node" => "Idea".into(),
        Some(t) if t == "todo" => "Subtask".into(),
        Some(_) => "Task".into(),
    }
}

pub fn init(w: &MainWindow) {
    let ws = w.global::<crate::WorkspaceUi>();
    with(|s| ws.set_media_cards(ModelRc::from(s.cards.clone())));
    ws.on_media_set_kind(|k| set_kind(&k));
    ws.on_media_set_scope(|k| set_scope(&k));
    ws.on_media_search(|q| search(&q));
    ws.on_media_upload(upload);
    ws.on_media_open(|id| open(&id));
    ws.on_media_get(|id| get(&id));
    ws.on_media_copy(|id| copy(&id));
    ws.on_media_jump(|id| jump(&id));
    ws.on_media_delete(|id| delete(&id));
    ws.on_lb_close(lb_close);
    ws.on_lb_download(|| { if let Some(id) = with(|s| s.lb.as_ref().map(|i| kit::s(i, "id"))) { get(&id); } });
    ws.on_lb_play(lb_play);
}

pub fn clear() { with(|s| { s.items.clear(); s.state = "loading".into(); }); show(); }

/// A new project: with none open, "This project" can only be empty, so the
/// library starts on All projects.
pub fn project_changed() {
    with(|s| { if kit::project().is_empty() && s.scope == "project" { s.scope = "all".into(); } s.items.clear(); s.state = "loading".into(); });
    show();
}

fn card(it: &Value, thumbs: &HashMap<String, Option<slint::Image>>) -> WorkspaceMediaCard {
    let id = kit::s(it, "id");
    let thumb = thumbs.get(&id).cloned().flatten();
    let ext = { let e = kit::s(it, "ext").trim_start_matches('.').to_string(); if e.is_empty() { kit::s(it, "kind") } else { e } }.to_uppercase();
    let link_state = if it.get("link").is_none_or(|l| l.is_null()) { "none" } else if kit::s(it, "linkStatus") == "deleted" { "deleted" } else { "ok" };
    let word = link_word(it);
    WorkspaceMediaCard {
        id: id.into(), name: kit::s(it, "originalName").into(), kind: kit::s(it, "kind").into(), ext: ext.into(),
        project: trunc(&kit::s(it, "projectName"), 16).into(),
        link: word.clone().into(),
        link_label: trunc(&{ let l = kit::s(it, "linkLabel"); if l.is_empty() { word } else { l } }, 22).into(),
        link_state: link_state.into(),
        has_path: !kit::s(it, "path").is_empty(),
        has_thumb: thumb.is_some(), thumb: thumb.unwrap_or_default(),
    }
}

fn show() {
    let (cards, state, empty, kind, scope, q, want) = with(|s| {
        let has_project = !kit::project().is_empty();
        let shown: Vec<&Value> = s.items.iter().filter(|it| matches(it, &s.q)).collect();
        let (state, empty) = if s.state == "loading" { ("loading", String::new()) }
            else if s.scope == "project" && !has_project { ("empty", "Select a project, or switch to All projects.".to_string()) }
            else if s.items.is_empty() { ("empty", "No media here yet. Upload files, or attach them to a Kanban task/subtask or a Mindmap idea.".to_string()) }
            else if shown.is_empty() { ("empty", format!("No media matches \u{201c}{}\u{201d}.", s.q)) }
            else { ("ok", String::new()) };
        let want: Vec<String> = shown.iter().filter(|it| kit::s(it, "kind") == "image" && !kit::s(it, "ext").eq_ignore_ascii_case(".svg")).map(|it| kit::s(it, "id")).filter(|id| !s.thumbs.contains_key(id)).collect();
        (shown.iter().map(|it| card(it, &s.thumbs)).collect::<Vec<_>>(), state, empty, s.kind.clone(), s.scope.clone(), s.q.clone(), want)
    });
    with(|s| kit::sync_rows(&s.cards, cards));
    g(|ws| {
        ws.set_media_state(state.into());
        ws.set_media_empty(empty.into());
        ws.set_media_kind(kind.into());
        ws.set_media_scope(scope.into());
        ws.set_media_has_project(!kit::project().is_empty());
        if ws.get_media_q().as_str() != q { ws.set_media_q(q.into()); }
    });
    for id in want { thumb(id); }
}

/// A picture's thumbnail (fetched with the sign-in, decoded off the UI
/// thread, kept small).
fn thumb(id: String) {
    with(|s| s.thumbs.insert(id.clone(), None));
    let Some(api) = kit::api() else { return };
    let path = format!("/api/media/raw/{}", kit::enc(&id));
    kit::bg(move || match api.get_bytes(&path) {
        Ok((200, bytes)) => image::load_from_memory(&bytes).ok().map(|img| {
            let img = if img.width() > 320 || img.height() > 320 { img.thumbnail(320, 320) } else { img };
            let rgba = img.to_rgba8();
            let mut pb = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(rgba.width(), rgba.height());
            pb.make_mut_bytes().copy_from_slice(rgba.as_raw());
            pb
        }),
        _ => None,
    }, move |pb| {
        if let Some(pb) = pb { with(|s| s.thumbs.insert(id, Some(slint::Image::from_rgba8(pb)))); show(); }
    });
}

/// GET the list for the scope and kind.
pub fn load() {
    let (scope, kind, gen) = with(|s| { s.gen += 1; (s.scope.clone(), s.kind.clone(), s.gen) });
    let project = kit::project();
    if scope == "project" && project.is_empty() { with(|s| { s.items.clear(); s.state = "ok".into(); }); return show(); }
    let path = list_url(&scope, &kind, &project);
    kit::call(move |api| api.get(&path), move |r| {
        if with(|s| s.gen) != gen { return; }
        let items = match r { Ok(v) if v.get("ok").is_some_and(kit::truthy) => v.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default(), _ => vec![] };
        with(|s| { s.items = items; s.state = "ok".into(); });
        show();
    });
}

pub fn set_kind(k: &str) { with(|s| s.kind = k.to_string()); load(); }
pub fn set_scope(k: &str) { with(|s| s.scope = k.to_string()); load(); }
pub fn search(q: &str) { with(|s| s.q = q.to_string()); show(); }

fn item(id: &str) -> Option<Value> { with(|s| s.items.iter().find(|i| kit::s(i, "id") == id).cloned()) }

/// Upload: the picker (several), each to /api/media/upload in this project
/// (base64), then one toast and the list again.
pub fn upload() {
    let project = kit::project();
    kit::bg(|| crate::testctl::take_pick().map(|p| vec![p]).or_else(|| rfd::FileDialog::new().pick_files()).unwrap_or_default(), move |paths| {
        if paths.is_empty() { return; }
        let project = project.clone();
        kit::call(move |api| {
            use base64::Engine;
            let mut ok = 0;
            let mut errs = vec![];
            for p in paths {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
                let bytes = match std::fs::read(&p) { Ok(b) => b, Err(e) => { errs.push(format!("Upload error: {e}")); continue } };
                let mut body = json!({ "name": name, "data": base64::engine::general_purpose::STANDARD.encode(bytes) });
                if !project.is_empty() { body["project"] = json!(project); }
                match api.post("/api/media/upload", &body) {
                    Ok(v) if v.get("ok").is_some_and(kit::truthy) => ok += 1,
                    Ok(v) => errs.push({ let e = kit::s(&v, "error"); if e.is_empty() { "Upload failed".into() } else { e } }),
                    Err(e) => errs.push(format!("Upload error: {e}")),
                }
            }
            Ok(json!({ "ok": ok, "errors": errs }))
        }, |r| {
            if let Ok(v) = r {
                for e in v.get("errors").and_then(|e| e.as_array()).cloned().unwrap_or_default() { kit::toast_error(e.as_str().unwrap_or("")); }
                let n = v.get("ok").and_then(|n| n.as_i64()).unwrap_or(0);
                if n > 0 { kit::toast_ok(&format!("Uploaded {n} file{}", if n == 1 { "" } else { "s" })); }
            }
            load();
        });
    });
}

/// View (openMediaLightbox): a picture in the lightbox; video, audio and
/// PDF too, with the way to the system's player; anything else downloads.
pub fn open(id: &str) {
    let Some(it) = item(id) else { return };
    let kind = kit::s(&it, "kind");
    if kind == "other" { return get(id); }
    let note = match kind.as_str() {
        "video" => "Videos play in the system's player.",
        "audio" => "Audio plays in the system's player.",
        "pdf" => "PDFs open in the system's viewer.",
        _ => "",
    };
    with(|s| s.lb = Some(it.clone()));
    g(|ws| {
        ws.set_lb_title(kit::s(&it, "originalName").into());
        ws.set_lb_kind(kind.clone().into());
        ws.set_lb_note(note.into());
        ws.set_lb_img(with(|s| s.thumbs.get(id).cloned().flatten()).unwrap_or_default());
        ws.set_lb_open(true);
    });
    if let Some(w) = kit::ui() { w.global::<crate::Shell>().set_modal_workspace(true); }
    if kind == "image" {
        let Some(api) = kit::api() else { return };
        let path = format!("/api/media/raw/{}", kit::enc(id));
        let id = id.to_string();
        kit::bg(move || match api.get_bytes(&path) {
            Ok((200, bytes)) => image::load_from_memory(&bytes).ok().map(|img| {
                let rgba = img.to_rgba8();
                let mut pb = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(rgba.width(), rgba.height());
                pb.make_mut_bytes().copy_from_slice(rgba.as_raw());
                pb
            }),
            _ => None,
        }, move |pb| {
            if with(|s| s.lb.as_ref().map(|i| kit::s(i, "id"))) != Some(id) { return; }
            if let Some(pb) = pb { g(|ws| ws.set_lb_img(slint::Image::from_rgba8(pb))); }
        });
    }
}

pub fn lb_close() {
    with(|s| s.lb = None);
    g(|ws| { ws.set_lb_open(false); ws.set_lb_img(slint::Image::default()); });
    if let Some(w) = kit::ui() { w.global::<crate::Shell>().set_modal_workspace(false); }
}

/// The lightbox's Play / Open: the file in the system's app.
pub fn lb_play() {
    let Some(it) = with(|s| s.lb.clone()) else { return };
    let id = kit::s(&it, "id");
    let name = format!("{}{}", id, kit::s(&it, "ext"));
    crate::app::open_fetched(format!("/api/media/raw/{}", kit::enc(&id)), name);
}

/// Get / Download: where to save, then the file.
pub fn get(id: &str) {
    let name = item(id).map(|i| kit::s(&i, "originalName")).filter(|n| !n.is_empty()).unwrap_or_else(|| "download".into());
    crate::app::download_url(format!("/api/media/raw/{}?download=1", kit::enc(id)), name);
}

pub fn copy(id: &str) {
    let Some(p) = item(id).map(|i| kit::s(&i, "path")).filter(|p| !p.is_empty()) else { return };
    super::note_copy(&p);
    if arboard::Clipboard::new().and_then(|mut c| c.set_text(p)).is_ok() { kit::toast("Path copied"); } else { kit::toast_error("Copy failed"); }
}

/// Jump to source (mediaJump): its project, then the Kanban or the Mind map.
pub fn jump(id: &str) {
    let Some(it) = item(id) else { return };
    if it.get("link").is_none_or(|l| l.is_null()) { return; }
    if kit::s(&it, "linkStatus") == "deleted" { return kit::toast_error("The original was deleted"); }
    lb_close();
    let p = kit::s(&it, "project");
    if !p.is_empty() && p != kit::project() {
        if kit::projects().iter().any(|x| kit::s(x, "alias") == p) { crate::app::select_project(&p); }
        else { return kit::toast_error("Source project not available"); }
    }
    let node = it.get("link").map(|l| kit::s(l, "type") == "node").unwrap_or(false);
    kit::show_tab(if node { "mindmap" } else { "kanban" });
}

/// Delete asks first.
pub fn delete(id: &str) {
    let id = id.to_string();
    kit::confirm(kit::Confirm { title: "Delete media", message: "Delete this media file? This permanently removes it and cannot be undone.", ok: "Delete", danger: true }, move |yes| {
        if !yes { return; }
        let body = json!({ "id": id });
        kit::call(move |api| api.post("/api/media/delete", &body), |r| {
            match r {
                Ok(v) if v.get("ok").is_some_and(kit::truthy) => kit::toast_ok("Deleted"),
                Ok(v) => kit::toast_error(&{ let e = kit::s(&v, "error"); if e.is_empty() { "Delete failed".into() } else { e } }),
                Err(e) => kit::toast_error(&format!("Error: {e}")),
            }
            load();
        });
    });
}

pub fn props() -> Value {
    with(|s| json!({
        "state": s.state, "kind": s.kind, "scope": s.scope, "q": s.q,
        "cards": s.cards.iter().map(|c| json!({ "id": c.id.as_str(), "name": c.name.as_str(), "kind": c.kind.as_str(), "thumb": c.has_thumb, "link": c.link_state.as_str() })).collect::<Vec<_>>(),
        "lightbox": s.lb.as_ref().map(|i| kit::s(i, "originalName")),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_and_filters() {
        assert_eq!(list_url("project", "all", "demo"), "/api/media/list?project=demo&includeGeneral=1");
        assert_eq!(list_url("all", "kanban", "demo"), "/api/media/list?kind=kanban");
        let it = json!({ "originalName": "Logo.PNG", "ext": ".png", "kind": "image", "projectName": "Demo" });
        assert!(matches(&it, "logo") && matches(&it, "demo") && matches(&it, "") && !matches(&it, "zip"));
        assert_eq!(trunc("abcdefghij", 5), "abcd\u{2026}");
        assert_eq!(link_word(&json!({ "link": { "type": "node" } })), "Idea");
        assert_eq!(link_word(&json!({})), "");
    }
}
