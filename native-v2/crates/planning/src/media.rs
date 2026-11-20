//! The Media modal (the web's openMediaModal + mountMediaBrowser in a
//! `#media-modal`, src/webapp-html.js): the Kanban's "Media" (kind kanban),
//! a task's or a subtask's media ("Task media", "Subtask media": only what
//! is linked to it, uploads linked to it), the Mindmap's "Media". The cards,
//! the search and the filters are Workspace's (crundi_workspace::media: the
//! search, the link word, the grid's columns); the modal is Planning's
//! (F44). One entity (`MediaModal`) holds the modal's state and is its body
//! in the one modal layer; the lightbox is drawn over it.

use crate::util::{self, enc, s, truthy};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use crundi_workspace::media::{cols_for, link_word, matches};
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{collections::HashMap, rc::Rc, sync::Arc};

/// What the modal shows (the web's state object).
#[derive(Clone, Default)]
pub struct Scope {
    /// "all" or "kanban" / "mindmap" / "unlinked" (the chips).
    pub kind: String,
    /// "project" or "all".
    pub scope: String,
    /// A task / subtask / idea (linkFilter; then no chips: "Attached here").
    pub link: Option<Value>,
    /// Uploads are linked to this (uploadLink) and go to this project (uploadProject).
    pub upload_link: Option<Value>,
    pub upload_project: Option<String>,
}

impl Scope {
    pub fn kind(kind: &str) -> Self { Scope { kind: kind.into(), scope: "project".into(), ..Default::default() } }
    /// One target's media (a task, a subtask, an idea), uploads linked to it.
    pub fn linked(link: Value, project: Option<String>) -> Self {
        Scope { kind: "all".into(), scope: "project".into(), link: Some(link.clone()), upload_link: Some(link), upload_project: project }
    }
}

/// mediaListUrl: the project's (with the general ones), a link's, or a kind's.
pub fn list_url(st: &Scope, project: &str) -> String {
    let mut p: Vec<String> = vec![];
    if st.scope == "project" && !project.is_empty() { p.push(format!("project={}", enc(project))); p.push("includeGeneral=1".into()); }
    if let Some(f) = &st.link {
        p.push(format!("linkType={}", enc(&s(f, "type"))));
        for k in ["taskId", "todoId", "nodeId"] { let v = s(f, k); if !v.is_empty() { p.push(format!("{k}={}", enc(&v))); } }
    } else if !st.kind.is_empty() && st.kind != "all" {
        p.push(format!("kind={}", st.kind));
    }
    format!("/api/media/list?{}", p.join("&"))
}

pub struct MediaModal {
    title: String,
    st: Scope,
    project: String,
    items: Vec<Value>,
    /// The cards on show (the search applied, once per change).
    shown: Vec<Value>,
    loaded: bool,
    q: String,
    search: Entity<w::InputState>,
    thumbs: HashMap<String, Option<Arc<Image>>>,
    lb: Option<Value>,
    lb_focus: FocusHandle,
    cols: usize,
    /// A grid row's height, measured once (the grid is as tall as its rows, up to 58% of the window).
    row_h: Option<Pixels>,
    grid: UniformListScrollHandle,
    /// What to load again after a change (the board's attachment strips).
    after: Option<Rc<dyn Fn(&mut App)>>,
    /// What shows again when it closes (the idea dialog it was opened from).
    back: Option<Back>,
    _sub: Subscription,
}

/// What to show again when a modal closes (the single modal layer: the
/// web stacks the media modal over the idea dialog).
pub type Back = Rc<dyn Fn(&mut Window, &mut App)>;

struct Handle(Entity<MediaModal>);
impl Global for Handle {}

fn get(cx: &App) -> Option<Entity<MediaModal>> { cx.try_global::<Handle>().map(|h| h.0.clone()) }

/// openMediaModal(title, state): the modal over the window. `after` runs
/// after an upload or a delete (the board's strips again).
pub fn open(title: &str, st: Scope, after: Option<Rc<dyn Fn(&mut App)>>, back: Option<Back>, cx: &mut App) {
    let title = title.to_string();
    cx.defer(move |cx| util::in_window(cx, move |window, cx| {
        let project = crundi_shell::project(cx);
        let e = cx.new(|cx| {
            let search = cx.new(|cx| w::InputState::new(window, cx).placeholder("Search media…"));
            let _sub = cx.subscribe_in(&search, window, |m: &mut MediaModal, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::Change = ev {
                let q = st.read(cx).value().to_string();
                if q != m.q { m.q = q; m.derive(cx); }
            });
            MediaModal { title, st, project, items: vec![], shown: vec![], loaded: false, q: String::new(), search, thumbs: HashMap::new(), lb: None,
                lb_focus: cx.focus_handle(), cols: 1, row_h: None, grid: UniformListScrollHandle::new(), after, back, _sub }
        });
        cx.set_global(Handle(e.clone()));
        e.update(cx, |m, cx| m.load(cx));
        w::modal(window, cx, T::MEDIA_MODAL_W, e.into(), |window, cx| {
            let Some(e) = get(cx) else { return };
            cx.remove_global::<Handle>();
            if let Some(b) = e.update(cx, |m, _| m.back.take()) { b(window, cx); }
        });
    }));
}

/// The modal is open (the driver's state).
pub fn props(cx: &App) -> Value {
    let Some(e) = get(cx) else { return Value::Null };
    if !w::modal_open(cx) { return Value::Null; }
    let m = e.read(cx);
    json!({
        "title": m.title, "loaded": m.loaded, "q": m.q, "kind": m.st.kind, "scope": m.st.scope, "attachedHere": m.st.link.is_some(),
        "cards": m.shown.iter().map(|c| { let id = s(c, "id"); json!({ "id": id, "name": s(c, "originalName"), "kind": s(c, "kind"),
            "thumb": m.thumbs.get(&id).is_some_and(|t| t.is_some()), "link": c.get("link").cloned().unwrap_or(Value::Null) }) }).collect::<Vec<_>>(),
        "empty": m.empty_text(),
        "lightbox": m.lb.as_ref().map(|i| s(i, "originalName")),
    })
}

/// The driver's actions on the open modal.
pub fn test_ui(name: &str, a: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    let arg = |i: usize| a.get(i).cloned().unwrap_or_default();
    let Some(e) = get(cx).filter(|_| w::modal_open(cx)) else {
        return name.starts_with("media-modal").then(|| json!({ "ok": false, "error": "no media modal" }));
    };
    match name {
        "media-modal-upload" => e.update(cx, |m, cx| m.upload(cx)),
        "media-modal-search" => { let q = arg(0); e.update(cx, |m, cx| { m.search.update(cx, |st, cx| st.set_value(q.clone(), window, cx)); m.q = q; m.derive(cx); }); }
        "media-modal-kind" => { let k = arg(0); e.update(cx, |m, cx| { m.st.kind = k; m.load(cx); }); }
        "media-modal-scope" => { let k = arg(0); e.update(cx, |m, cx| { m.st.scope = k; m.load(cx); }); }
        "media-modal-view" => { let id = arg(0); e.update(cx, |m, cx| m.view(&id, cx)); }
        "media-modal-copy" => { let id = arg(0); e.update(cx, |m, cx| m.copy(&id, cx)); }
        "media-modal-delete" => { let id = arg(0); e.update(cx, |m, cx| m.delete(&id, window, cx)); }
        "media-modal-jump" => { let id = arg(0); e.update(cx, |m, cx| m.jump(&id, window, cx)); }
        "media-modal-lb-close" => e.update(cx, |m, cx| { m.lb = None; cx.notify(); }),
        "media-modal-close" => w::close_dialog(window, cx),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

impl MediaModal {
    fn derive(&mut self, cx: &mut Context<Self>) {
        self.shown = self.items.iter().filter(|it| matches(it, &self.q)).cloned().collect();
        cx.notify();
    }

    /// The list again (the web's reload): nothing when "This project" has no project.
    fn load(&mut self, cx: &mut Context<Self>) {
        self.project = crundi_shell::project(cx);
        if self.st.scope == "project" && self.project.is_empty() {
            self.items.clear(); self.loaded = true; self.derive(cx); return;
        }
        let path = list_url(&self.st, &self.project);
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
            let _ = me.update(cx, |m, cx| {
                m.items = r.ok().filter(|v| v.get("ok").is_some_and(truthy)).and_then(|v| v.get("items").and_then(|i| i.as_array()).cloned()).unwrap_or_default();
                m.loaded = true;
                m.thumbs_wanted(cx);
                m.derive(cx);
            });
        });
    }

    fn thumbs_wanted(&mut self, cx: &mut Context<Self>) {
        let want: Vec<String> = self.items.iter().filter(|it| s(it, "kind") == "image").map(|it| s(it, "id")).filter(|id| !self.thumbs.contains_key(id)).collect();
        for id in want {
            self.thumbs.insert(id.clone(), None);
            let Some(api) = crundi_shell::api(cx) else { return };
            let path = format!("/api/media/raw/{}", enc(&id));
            let me = cx.entity().downgrade();
            crundi_shell::run(cx, move || match api.get_bytes(&path) { Ok((200, b)) => Some(b), _ => None }, move |b, cx| {
                let Some(b) = b else { return };
                let Some(f) = util::image_format(&b) else { return };
                let _ = me.update(cx, |m, cx| { m.thumbs.insert(id, Some(Arc::new(Image::from_bytes(f, b)))); cx.notify(); });
            });
        }
    }

    fn item(&self, id: &str) -> Option<Value> { self.items.iter().find(|i| s(i, "id") == id).cloned() }

    /// After a change: this list, and what the opener shows (refreshAllMediaViews, loadKanban).
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.load(cx);
        if let Some(f) = self.after.clone() { cx.defer(move |cx| f(cx)); }
    }

    /// mediaTriggerUpload + mediaUploadFiles: linked to the target, in its project.
    fn upload(&mut self, cx: &mut Context<Self>) {
        let link = self.st.upload_link.clone();
        let project = self.st.upload_project.clone().filter(|p| !p.is_empty())
            .or_else(|| if link.is_none() { Some(crundi_shell::project(cx)).filter(|p| !p.is_empty()) } else { None });
        let Some(api) = crundi_shell::api(cx) else { return };
        let me = cx.entity().downgrade();
        crundi_shell::run(cx, move || {
            use base64::Engine;
            let paths = crundi_core::driver::take_pick().map(|p| vec![p]).or_else(|| rfd::FileDialog::new().pick_files()).unwrap_or_default();
            let (mut ok, mut errs) = (0, vec![]);
            for p in paths {
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
                let bytes = match std::fs::read(&p) { Ok(b) => b, Err(e) => { errs.push(format!("Upload error: {e}")); continue } };
                let mut body = json!({ "name": name, "data": base64::engine::general_purpose::STANDARD.encode(bytes) });
                if let Some(l) = &link { body["link"] = l.clone(); }
                if let Some(p) = &project { body["project"] = json!(p); }
                match api.post("/api/media/upload", &body) {
                    Ok(v) if v.get("ok").is_some_and(truthy) => ok += 1,
                    Ok(v) => errs.push({ let e = s(&v, "error"); if e.is_empty() { "Upload failed".into() } else { e } }),
                    Err(e) => errs.push(format!("Upload error: {e}")),
                }
            }
            (ok, errs)
        }, move |(ok, errs), cx| {
            for e in errs { util::toast(cx, &e, w::Toast::Error); }
            if ok > 0 { util::toast(cx, &format!("Uploaded {ok} file{}", if ok == 1 { "" } else { "s" }), w::Toast::Success); }
            let _ = me.update(cx, |m, cx| m.changed(cx));
        });
    }

    /// View: the lightbox (anything not a picture, video, audio or PDF downloads).
    fn view(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(it) = self.item(id) else { return };
        if s(&it, "kind") == "other" { return self.download(id, cx); }
        self.lb = Some(it);
        cx.notify();
        // Esc closes the lightbox first (its own keys).
        let f = self.lb_focus.clone();
        cx.defer(move |cx| util::in_window(cx, move |window, cx| f.focus(window, cx)));
    }

    fn download(&mut self, id: &str, cx: &mut Context<Self>) {
        let name = self.item(id).map(|i| s(&i, "originalName")).unwrap_or_default();
        save(id, &name, cx);
    }

    fn copy(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(p) = self.item(id).map(|i| s(&i, "path")).filter(|p| !p.is_empty()) else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(p));
        util::toast(cx, "Path copied", w::Toast::Info);
    }

    /// mediaJump: the modal closes; its project, then the Kanban or the Mind map.
    fn jump(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(it) = self.item(id) else { return };
        if it.get("link").is_none_or(|l| l.is_null()) { return; }
        if s(&it, "linkStatus") == "deleted" { return util::toast(cx, "The original was deleted", w::Toast::Error); }
        self.lb = None;
        self.back = None;
        w::close_dialog(window, cx);
        let p = s(&it, "project");
        let stores = crundi_shell::stores(cx);
        if !p.is_empty() && p != crundi_shell::project(cx) { stores.projects.update(cx, |x, cx| x.pick(&p, cx)); }
        let node = it.get("link").map(|l| s(l, "type") == "node").unwrap_or(false);
        util::show_tab(cx, if node { "mindmap" } else { "kanban" });
    }

    fn delete(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let id = id.to_string();
        let me = cx.entity().downgrade();
        let c = w::Confirm { title: "Delete media".into(), message: "Delete this media file? This permanently removes it and cannot be undone.".into(), ok: "Delete".into(), danger: true };
        w::confirm(window, cx, c, move |_, cx| {
            let body = json!({ "id": id });
            let me = me.clone();
            crundi_shell::call(cx, move |api| api.post("/api/media/delete", &body), move |r, cx| {
                match r {
                    Ok(v) if v.get("ok").is_some_and(truthy) => util::toast(cx, "Deleted", w::Toast::Success),
                    Ok(v) => { let e = s(&v, "error"); util::toast(cx, if e.is_empty() { "Delete failed" } else { &e }, w::Toast::Error) }
                    Err(e) => util::toast(cx, &format!("Error: {e}"), w::Toast::Error),
                }
                let _ = me.update(cx, |m, cx| m.changed(cx));
            });
        });
    }

    fn empty_text(&self) -> Option<String> {
        if !self.loaded { return None; }
        if self.st.link.is_none() && self.st.scope == "project" && self.project.is_empty() { return Some("Select a project, or switch to All projects.".into()); }
        if self.items.is_empty() { return Some("No media here yet. Upload files, or attach them to a Kanban task/subtask or a Mindmap idea.".into()); }
        if self.shown.is_empty() { return Some(format!("No media matches \u{201c}{}\u{201d}.", self.q)); }
        None
    }

    /// mediaCardHtml (Workspace's card, the same look).
    fn card(&self, it: &Value, i: usize, cx: &mut Context<Self>) -> AnyElement {
        let id = s(it, "id");
        let kind = s(it, "kind");
        let ext = { let e = s(it, "ext").trim_start_matches('.').to_string(); if e.is_empty() { kind.clone() } else { e } }.to_uppercase();
        let thumb = self.thumbs.get(&id).cloned().flatten();
        let linked = it.get("link").is_some_and(|l| !l.is_null());
        let deleted = s(it, "linkStatus") == "deleted";
        let sub = [s(it, "projectName"), if linked { format!("{} {}", link_word(it), s(it, "linkLabel")) } else { "Unlinked".into() }].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
        let glyph = match kind.as_str() { "video" => IconName::Video, "audio" => IconName::Music, "pdf" => IconName::FileText, "image" => IconName::Image, _ => IconName::File };
        let has_path = !s(it, "path").is_empty();
        let (a, b, c, d) = (id.clone(), id.clone(), id.clone(), id.clone());
        w::card().w(T::WS_MEDIA_CARD).overflow_hidden()
            .child(div().id(("mm-media-thumb", i)).h(T::WS_MEDIA_THUMB).w_full().flex().items_center().justify_center().bg(T::bg_secondary()).cursor_pointer()
                .on_click(cx.listener(move |m, _, _, cx| m.view(&a, cx)))
                .child(match thumb {
                    Some(img) => gpui::img(img).size_full().object_fit(ObjectFit::Cover).into_any_element(),
                    None => div().flex().flex_col().items_center().gap(T::SP_4).text_color(T::text_muted())
                        .child(w::icon(glyph, T::ICON_XL)).child(div().text_size(T::TEXT_10).font_weight(T::W_BOLD).child(ext)).into_any_element(),
                }))
            .child(div().flex().flex_col().gap(T::SP_2).p(T::SP_8)
                .child(div().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text()).truncate().child(s(it, "originalName")))
                .child(div().text_size(T::TEXT_10).text_color(if deleted { T::text_muted() } else if linked { T::accent_hover() } else { T::text_muted() }).truncate().child(sub))
                .child(div().flex().gap(T::SP_4).pt(T::SP_4)
                    .child(w::button(("mm-media-view", i), if kind == "other" { "Get" } else { "View" }).icon(if kind == "other" { IconName::Download } else { IconName::Eye })
                        .on_click(cx.listener(move |m, _, _, cx| m.view(&b, cx))))
                    .when(has_path, |d0| d0.child(w::icon_button(("mm-media-copy", i), IconName::Copy, "Copy file path").on_click(cx.listener(move |m, _, _, cx| m.copy(&c, cx)))))
                    .when(linked, |d0| { let j = id.clone(); d0.child(w::disabled(w::icon_button(("mm-media-jump", i), IconName::ExternalLink, "Jump to source"), deleted).on_click(cx.listener(move |m, _, window, cx| m.jump(&j, window, cx)))) })
                    .child(w::icon_button(("mm-media-del", i), IconName::Trash, "Delete").on_click(cx.listener(move |m, _, window, cx| m.delete(&d, window, cx))))))
            .into_any_element()
    }

    /// The lightbox over the modal (#media-lightbox).
    fn lightbox(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let it = self.lb.clone()?;
        let id = s(&it, "id");
        let thumb = self.thumbs.get(&id).cloned().flatten();
        let close: Rc<dyn Fn(&mut Window, &mut App)> = { let me = cx.entity().downgrade(); Rc::new(move |_, cx| { let _ = me.update(cx, |m, cx| { m.lb = None; cx.notify(); }); }) };
        let key = close.clone();
        let m = w::Modal {
            id: "media-modal-lb", width: T::WS_DIFF_W, focus: self.lb_focus.clone(), scroll: ScrollHandle::new(),
            on_close: close.clone(),
            on_key: Rc::new(move |e: &KeyDownEvent, window, cx| if e.keystroke.key == "escape" { cx.stop_propagation(); key(window, cx); }),
        };
        let dl: Rc<dyn Fn(&mut App)> = { let me = cx.entity().downgrade(); Rc::new(move |cx| { let id = id.clone(); let _ = me.update(cx, |m, cx| m.download(&id, cx)); }) };
        Some(w::overlay_modal(m, window, vec![lb_body(&it, thumb, close, dl)]).into_any_element())
    }
}

/// mediaDownload: where to save, then the file.
fn save(id: &str, name: &str, cx: &mut App) {
    let name = if name.is_empty() { "download".to_string() } else { name.to_string() };
    let url = format!("/api/media/raw/{}?download=1", enc(id));
    let Some(api) = crundi_shell::api(cx) else { return };
    crundi_shell::run(cx, move || -> Option<Result<String, String>> {
        let path = crundi_core::driver::take_pick().or_else(|| rfd::FileDialog::new().set_title(format!("Save {name}")).set_file_name(&name).save_file())?;
        Some(match api.get_bytes(&url) {
            Ok((200, bytes)) => std::fs::write(&path, bytes).map(|_| name.clone()).map_err(|e| e.to_string()),
            Ok(_) => Err("Could not download".into()),
            Err(e) => Err(e.to_string()),
        })
    }, |r, cx| match r {
        Some(Ok(n)) => util::toast(cx, &format!("Saved {n}"), w::Toast::Success),
        Some(Err(e)) => util::toast(cx, &e, w::Toast::Error),
        None => {}
    });
}

/// The lightbox's content (openMediaLightbox): the name and its ×, the
/// picture (a video, audio or PDF says where it opens), Download.
fn lb_body(it: &Value, thumb: Option<Arc<Image>>, close: Rc<dyn Fn(&mut Window, &mut App)>, dl: Rc<dyn Fn(&mut App)>) -> AnyElement {
    let note = match s(it, "kind").as_str() { "video" => "Videos play in the system's player.", "audio" => "Audio plays in the system's player.", "pdf" => "PDFs open in the system's viewer.", _ => "" };
    div().flex().flex_col().w_full().h(T::WS_DIFF_H.min(T::DIALOG_MAX_H - T::SP_12))
        .child(w::modal_head("media-lb-x", s(it, "originalName"), false, move |_, window, cx| close(window, cx)))
        .child(div().flex_1().min_h_0().flex().items_center().justify_center().bg(T::bg()).p(T::SP_12)
            .child(match thumb { Some(img) => gpui::img(img).max_w_full().max_h_full().object_fit(ObjectFit::Contain).into_any_element(), None => w::empty(if note.is_empty() { "Loading…" } else { note }).into_any_element() }))
        .child(w::toolbar().px(T::SP_16).py(T::SP_10).child(div().flex_1())
            .child(w::button("media-lb-dl", "Download").icon(IconName::Download).on_click(move |_, _, cx| dl(cx))))
        .into_any_element()
}

/// A lightbox on its own (a card's attachment strip): the modal layer's body.
struct Lightbox { it: Value, thumb: Option<Arc<Image>> }

/// openMediaLightbox(it): anything not viewable downloads instead.
pub fn lightbox(it: Value, back: Option<Back>, cx: &mut App) {
    cx.defer(move |cx| util::in_window(cx, move |window, cx| {
        let id = s(&it, "id");
        if s(&it, "kind") == "other" { return save(&id, &s(&it, "originalName"), cx); }
        let e = cx.new(|_| Lightbox { it: it.clone(), thumb: None });
        if s(&it, "kind") == "image" {
            if let Some(api) = crundi_shell::api(cx) {
                let path = format!("/api/media/raw/{}", enc(&id));
                let me = e.downgrade();
                crundi_shell::run(cx, move || match api.get_bytes(&path) { Ok((200, b)) => Some(b), _ => None }, move |b, cx| {
                    let Some(b) = b else { return };
                    let Some(f) = util::image_format(&b) else { return };
                    let _ = me.update(cx, |l, cx| { l.thumb = Some(Arc::new(Image::from_bytes(f, b))); cx.notify(); });
                });
            }
        }
        w::modal(window, cx, T::WS_DIFF_W, e.into(), move |window, cx| { if let Some(b) = &back { b(window, cx); } });
    }));
}

impl Render for Lightbox {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let it = self.it.clone();
        let id = s(&it, "id");
        let name = s(&it, "originalName");
        let dl: Rc<dyn Fn(&mut App)> = Rc::new(move |cx| save(&id, &name, cx));
        crundi_core::driver::rendered("media-lightbox");
        let _ = cx;
        lb_body(&it, self.thumb.clone(), Rc::new(|window, cx| w::close_dialog(window, cx)), dl)
    }
}

impl Render for MediaModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("media-modal");
        let has_project = !self.project.is_empty();
        // The toolbar (.media-toolbar): Upload, the search, then the chips
        // or, for one target, "Attached here".
        let mut bar = w::toolbar().flex_wrap().border_b_1().border_color(T::border())
            .child(w::primary_icon("media-modal-upload", IconName::Upload, "Upload").on_click(cx.listener(|m, _, _, cx| m.upload(cx))))
            .child(div().flex_none().w(T::WS_MEDIA_SEARCH).child(w::input(&self.search)));
        if self.st.link.is_some() {
            bar = bar.child(div().flex_1()).child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child("Attached here"));
        } else {
            let chip = |id: &'static str, label: &'static str, k: &'static str, on: bool, cx: &mut Context<Self>| w::toggle(id, label, on).on_click(cx.listener(move |m, _, _, cx| { m.st.kind = k.into(); m.load(cx); }));
            let kind = self.st.kind.clone();
            bar = bar.child(chip("mmk-all", "All", "all", kind == "all", cx)).child(chip("mmk-kanban", "Kanban", "kanban", kind == "kanban", cx))
                .child(chip("mmk-mindmap", "Mindmap", "mindmap", kind == "mindmap", cx)).child(chip("mmk-unlinked", "Unlinked", "unlinked", kind == "unlinked", cx))
                .child(div().flex_1())
                .when(has_project, |b| b.child(w::toggle("mms-project", "This project", self.st.scope == "project").on_click(cx.listener(|m, _, _, cx| { m.st.scope = "project".into(); m.load(cx); }))))
                .child(w::toggle("mms-all", "All projects", self.st.scope == "all").on_click(cx.listener(|m, _, _, cx| { m.st.scope = "all".into(); m.load(cx); })));
        }
        // The body (.media-body: up to 58% of the window, then it scrolls).
        let max_h = (window.viewport_size().height * 0.58).min(T::DIALOG_MAX_H - T::SP_48 * 3.).max(T::WS_MEDIA_THUMB);
        let body: AnyElement = match (self.loaded, self.empty_text()) {
            (false, _) => div().p(T::SP_16).child(w::loading("Loading…")).into_any_element(),
            (_, Some(t)) => div().p(T::SP_16).text_size(T::TEXT_13).text_color(T::text_muted()).child(t).into_any_element(),
            _ => {
                let me = cx.entity();
                let measure = canvas(move |b, _, cx| {
                    let n = cols_for(f32::from(b.size.width));
                    if me.read(cx).cols != n { let me = me.clone(); cx.defer(move |cx| me.update(cx, |m, cx| { m.cols = n; cx.notify(); })); }
                }, |_, _, _, _| {}).absolute().top_0().left_0().size_full();
                let cols = self.cols.max(1);
                let rows = self.shown.len().div_ceil(cols);
                let h = self.row_h.map(|r| (r * rows as f32 + T::SP_12).min(max_h)).unwrap_or(max_h);
                let grid = uniform_list("media-modal-grid", rows, cx.processor(move |m, range: std::ops::Range<usize>, _, cx| {
                    range.map(|r| {
                        let mut row = div().w_full().flex().gap(T::SP_12).px(T::SP_12).pt(T::SP_12);
                        for i in r * cols..((r + 1) * cols).min(m.shown.len()) { let it = m.shown[i].clone(); row = row.child(m.card(&it, i, cx)); }
                        if r == 0 && m.row_h.is_none() {
                            let me = cx.entity();
                            row = row.child(canvas(move |b, _, cx| {
                                let hh = b.size.height;
                                if hh > px(0.) { let me = me.clone(); cx.defer(move |cx| me.update(cx, |m, cx| if m.row_h.is_none() { m.row_h = Some(hh); cx.notify(); })); }
                            }, |_, _, _, _| {}).absolute().top_0().left_0().size_full());
                            row = row.relative();
                        }
                        row.into_any_element()
                    }).collect::<Vec<_>>()
                })).track_scroll(&self.grid).size_full();
                div().relative().w_full().h(h).child(measure).child(grid).into_any_element()
            }
        };
        let title = self.title.clone();
        div().flex().flex_col().w_full()
            .child(w::modal_head("media-modal-x", title, false, |_, window, cx| w::close_dialog(window, cx)))
            .child(div().mx(T::SP_8).flex().flex_col().border_1().border_color(T::border()).rounded(T::R_CARD).overflow_hidden().child(bar).child(body))
            .child(div().flex().justify_end().p(T::SP_8).pt(T::SP_12)
                .child(w::button("media-modal-close", "Close").on_click(|_, window, cx| w::close_dialog(window, cx))))
            .children(self.lightbox(window, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::{list_url, Scope};
    use serde_json::json;

    #[test]
    fn media_list_urls() {
        assert_eq!(list_url(&Scope::kind("kanban"), "demo"), "/api/media/list?project=demo&includeGeneral=1&kind=kanban");
        let t = Scope::linked(json!({ "type": "todo", "taskId": "t1", "todoId": "d2" }), Some("demo".into()));
        assert_eq!(list_url(&t, "demo"), "/api/media/list?project=demo&includeGeneral=1&linkType=todo&taskId=t1&todoId=d2");
        assert_eq!(list_url(&Scope::kind("all"), ""), "/api/media/list?");
    }
}
