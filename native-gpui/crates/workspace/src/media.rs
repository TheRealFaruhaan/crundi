//! Media (phase 3.5; the web's renderMediaBrowser and the media* actions,
//! the Slint client's src/workspace/media.rs): Upload, the search, All /
//! Kanban / Mindmap / Unlinked, This project / All projects; the cards
//! (thumbnail or kind and extension, the name, project and link line, View
//! or Get, Copy path, Jump to source, Delete); the lightbox for a picture
//! (video, audio and PDF say they open in the system's app).

use crate::toast;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};

fn s(v: &Value, k: &str) -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() }
fn truthy(v: &Value) -> bool { v.as_bool().unwrap_or(!v.is_null()) }

pub fn list_url(scope: &str, kind: &str, project: &str) -> String {
    let mut p: Vec<String> = vec![];
    if scope == "project" && !project.is_empty() { p.push(format!("project={}", crate::enc(project))); p.push("includeGeneral=1".into()); }
    if kind != "all" && !kind.is_empty() { p.push(format!("kind={kind}")); }
    format!("/api/media/list?{}", p.join("&"))
}

/// The search (mediaFilter): name, link label, project, extension, kind.
pub fn matches(it: &Value, q: &str) -> bool {
    let q = q.trim().to_lowercase();
    q.is_empty() || ["originalName", "linkLabel", "projectName", "ext", "kind"].iter().any(|k| s(it, k).to_lowercase().contains(&q))
}

/// A card's link line word (mediaLinkSubHtml): Idea / Subtask / Task.
pub fn link_word(it: &Value) -> String {
    match it.get("link").filter(|l| !l.is_null()).map(|l| s(l, "type")) {
        None => String::new(),
        Some(t) if t == "node" => "Idea".into(),
        Some(t) if t == "todo" => "Subtask".into(),
        Some(_) => "Task".into(),
    }
}

pub struct MediaView {
    pub kind: String,
    pub scope: String,
    pub items: Vec<Value>,
    pub state: String,
    err: String,
    thumbs: HashMap<String, Option<Arc<Image>>>,
    pub lb: Option<Value>,
    search: Entity<w::InputState>,
    q: String,
    _sub: Subscription,
}

pub struct MediaHandle(pub Entity<MediaView>);
impl Global for MediaHandle {}
pub fn view(cx: &App) -> Option<Entity<MediaView>> { cx.try_global::<MediaHandle>().map(|h| h.0.clone()) }

impl MediaView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| w::InputState::new(window, cx).placeholder("Search media…"));
        let _sub = cx.subscribe_in(&search, window, |m, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::Change = ev { m.q = st.read(cx).value().to_string(); cx.notify(); });
        MediaView { kind: "all".into(), scope: "project".into(), items: vec![], state: "loading".into(), err: String::new(), thumbs: HashMap::new(), lb: None, search, q: String::new(), _sub }
    }

    pub fn shown(&self) -> Vec<Value> { self.items.iter().filter(|it| matches(it, &self.q)).cloned().collect() }

    pub fn set_search(&mut self, q: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.q = q.to_string();
        self.search.update(cx, |st, cx| st.set_value(q.to_string(), window, cx));
        cx.notify();
    }

    /// GET the list for the scope and kind; a failed load says so (Retry).
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let project = crundi_shell::project(cx);
        if project.is_empty() && self.scope == "project" { self.scope = "all".into(); }
        let path = list_url(&self.scope, &self.kind, &project);
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
            let _ = me.update(cx, |m, cx| {
                match r {
                    Ok(v) if v.get("ok").is_some_and(truthy) => { m.items = v.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default(); m.state = "ok".into(); m.err.clear(); }
                    Ok(v) => { m.items.clear(); m.state = "error".into(); m.err = { let e = s(&v, "error"); if e.is_empty() { "Could not load media".into() } else { e } }; }
                    Err(e) => { m.items.clear(); m.state = "error".into(); m.err = format!("Could not load media: {e}"); }
                }
                m.thumbs_wanted(cx);
                cx.notify();
            });
        });
    }

    /// Each picture's thumbnail, fetched once.
    fn thumbs_wanted(&mut self, cx: &mut Context<Self>) {
        let want: Vec<(String, String)> = self.items.iter().filter(|it| s(it, "kind") == "image").map(|it| (s(it, "id"), s(it, "ext"))).filter(|(id, _)| !self.thumbs.contains_key(id)).collect();
        for (id, ext) in want {
            self.thumbs.insert(id.clone(), None);
            let Some(api) = crundi_shell::api(cx) else { return };
            let path = format!("/api/media/raw/{}", crate::enc(&id));
            let format = match ext.trim_start_matches('.').to_ascii_lowercase().as_str() { "jpg" | "jpeg" => ImageFormat::Jpeg, "gif" => ImageFormat::Gif, "webp" => ImageFormat::Webp, "svg" => ImageFormat::Svg, _ => ImageFormat::Png };
            let me = cx.entity().downgrade();
            crundi_shell::run(cx, move || match api.get_bytes(&path) { Ok((200, b)) => Some(b), _ => None }, move |b, cx| {
                if let Some(b) = b { let _ = me.update(cx, |m, cx| { m.thumbs.insert(id, Some(Arc::new(Image::from_bytes(format, b)))); cx.notify(); }); }
            });
        }
    }

    pub fn set_kind(&mut self, k: &str, cx: &mut Context<Self>) { self.kind = k.into(); self.load(cx); }
    pub fn set_scope(&mut self, k: &str, cx: &mut Context<Self>) { self.scope = k.into(); self.load(cx); }

    fn item(&self, id: &str) -> Option<Value> { self.items.iter().find(|i| s(i, "id") == id).cloned() }

    /// Upload: the picker, each to /api/media/upload in this project (base64), one toast, the list again.
    pub fn upload(&mut self, cx: &mut Context<Self>) {
        let project = crundi_shell::project(cx);
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
                if !project.is_empty() { body["project"] = json!(project); }
                match api.post("/api/media/upload", &body) {
                    Ok(v) if v.get("ok").is_some_and(truthy) => ok += 1,
                    Ok(v) => errs.push({ let e = s(&v, "error"); if e.is_empty() { "Upload failed".into() } else { e } }),
                    Err(e) => errs.push(format!("Upload error: {e}")),
                }
            }
            (ok, errs)
        }, move |(ok, errs), cx| {
            for e in errs { toast(cx, e, w::Toast::Error); }
            if ok > 0 { toast(cx, format!("Uploaded {ok} file{}", if ok == 1 { "" } else { "s" }), w::Toast::Success); }
            let _ = me.update(cx, |m, cx| m.load(cx));
        });
    }

    /// View: a picture (or video, audio, PDF) in the lightbox; anything else downloads.
    pub fn open(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(it) = self.item(id) else { return };
        if s(&it, "kind") == "other" { return self.get(id, cx); }
        if s(&it, "kind") == "image" && !self.thumbs.contains_key(id) { self.thumbs_wanted(cx); }
        self.lb = Some(it);
        cx.notify();
        let me = cx.entity();
        crate::with_window(cx, move |window, cx| {
            let lb = cx.new(|_| Lightbox(me.clone()));
            w::modal(window, cx, T::WS_DIFF_W, lb.into(), move |_, cx| { let _ = me.update(cx, |m, cx| { m.lb = None; cx.notify(); }); });
        });
    }

    /// Get / Download: where to save, then the file.
    pub fn get(&mut self, id: &str, cx: &mut Context<Self>) {
        let name = self.item(id).map(|i| s(&i, "originalName")).filter(|n| !n.is_empty()).unwrap_or_else(|| "download".into());
        crate::save_url(format!("/api/media/raw/{}?download=1", crate::enc(id)), name, cx);
    }

    pub fn copy(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(p) = self.item(id).map(|i| s(&i, "path")).filter(|p| !p.is_empty()) else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(p.clone()));
        crate::note_copy(cx, p);
        toast(cx, "Path copied".into(), w::Toast::Info);
    }

    /// Jump to source: its project, then the Kanban or the Mind map.
    pub fn jump(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(it) = self.item(id) else { return };
        if it.get("link").is_none_or(|l| l.is_null()) { return; }
        if s(&it, "linkStatus") == "deleted" { return toast(cx, "The original was deleted".into(), w::Toast::Error); }
        if self.lb.take().is_some() { w::close_dialog(window, cx); }
        let p = s(&it, "project");
        if !p.is_empty() && p != crundi_shell::project(cx) { crundi_shell::session::select_project(cx, &p); }
        let node = it.get("link").map(|l| s(l, "type") == "node").unwrap_or(false);
        if let Some(sh) = crundi_shell::shell(cx) { sh.update(cx, |sh, cx| sh.show_tab(if node { "mindmap" } else { "kanban" }, cx)); }
    }

    /// Delete asks first.
    pub fn delete(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let id = id.to_string();
        let me = cx.entity().downgrade();
        let c = w::Confirm { title: "Delete media".into(), message: "Delete this media file? This permanently removes it and cannot be undone.".into(), ok: "Delete".into(), danger: true };
        w::confirm(window, cx, c, move |_, cx| {
            let body = json!({ "id": id });
            let me = me.clone();
            crundi_shell::call(cx, move |api| api.post("/api/media/delete", &body), move |r, cx| {
                match r {
                    Ok(v) if v.get("ok").is_some_and(truthy) => toast(cx, "Deleted".into(), w::Toast::Success),
                    Ok(v) => { let e = s(&v, "error"); toast(cx, if e.is_empty() { "Delete failed".into() } else { e }, w::Toast::Error) }
                    Err(e) => toast(cx, format!("Error: {e}"), w::Toast::Error),
                }
                let _ = me.update(cx, |m, cx| m.load(cx));
            });
        });
    }

    pub fn props(&self) -> Value {
        json!({
            "state": self.state, "error": self.err, "kind": self.kind, "scope": self.scope, "q": self.q,
            "cards": self.shown().iter().map(|c| { let id = s(c, "id"); json!({ "id": id, "name": s(c, "originalName"), "kind": s(c, "kind"),
                "thumb": self.thumbs.get(&id).is_some_and(|t| t.is_some()), "link": if c.get("link").is_none_or(|l| l.is_null()) { "none" } else if s(c, "linkStatus") == "deleted" { "deleted" } else { "ok" } }) }).collect::<Vec<_>>(),
            "lightbox": self.lb.as_ref().map(|i| s(i, "originalName")),
        })
    }

    fn card(&self, it: &Value, i: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let id = s(it, "id");
        let kind = s(it, "kind");
        let ext = { let e = s(it, "ext").trim_start_matches('.').to_string(); if e.is_empty() { kind.clone() } else { e } }.to_uppercase();
        let thumb = self.thumbs.get(&id).cloned().flatten();
        let linked = it.get("link").is_some_and(|l| !l.is_null());
        let deleted = s(it, "linkStatus") == "deleted";
        let sub = [s(it, "projectName"), if linked { format!("{} {}", link_word(it), s(it, "linkLabel")) } else { "Unlinked".into() }].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
        let glyph = match kind.as_str() { "video" => IconName::Video, "audio" => IconName::Music, "pdf" => IconName::FileText, "image" => IconName::Image, _ => IconName::File };
        let (a, b, c, d) = (id.clone(), id.clone(), id.clone(), id.clone());
        w::card().w(T::WS_MEDIA_CARD).overflow_hidden()
            .child(div().id(("media-thumb", i)).h(T::WS_MEDIA_THUMB).w_full().flex().items_center().justify_center().bg(T::bg_secondary()).cursor_pointer()
                .on_click(cx.listener(move |m, _, _, cx| m.open(&a, cx)))
                .child(match thumb {
                    Some(img) => gpui::img(img).size_full().object_fit(ObjectFit::Cover).into_any_element(),
                    None => div().flex().flex_col().items_center().gap(T::SP_4).text_color(T::text_muted())
                        .child(w::icon(glyph, T::ICON_XL)).child(div().text_size(T::TEXT_10).font_weight(T::W_BOLD).child(ext)).into_any_element(),
                }))
            .child(div().flex().flex_col().gap(T::SP_2).p(T::SP_8)
                .child(div().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text()).truncate().child(s(it, "originalName")))
                .child(div().text_size(T::TEXT_10).text_color(if deleted { T::text_muted() } else if linked { T::accent_hover() } else { T::text_muted() }).truncate().child(sub))
                .child(div().flex().gap(T::SP_4).pt(T::SP_4)
                    .child(w::button(("media-view", i), if kind == "other" { "Get" } else { "View" }).icon(if kind == "other" { IconName::Download } else { IconName::Eye })
                        .on_click(cx.listener(move |m, _, _, cx| m.open(&b, cx))))
                    .child(w::icon_button(("media-copy", i), IconName::Copy, "Copy path").on_click(cx.listener(move |m, _, _, cx| m.copy(&c, cx))))
                    .when(linked, |d0| { let j = id.clone(); d0.child(w::disabled(w::icon_button(("media-jump", i), IconName::ExternalLink, "Jump to source"), deleted).on_click(cx.listener(move |m, _, window, cx| m.jump(&j, window, cx)))) })
                    .child(w::icon_button(("media-del", i), IconName::Trash, "Delete").on_click(cx.listener(move |m, _, window, cx| m.delete(&d, window, cx))))))
    }
}

/// The lightbox's view (in the one modal): the title, the picture (or what
/// a video, audio or PDF does), Download, Close.
struct Lightbox(Entity<MediaView>);

impl Render for Lightbox {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let m = self.0.read(cx);
        let Some(it) = m.lb.clone() else { return div().into_any_element() };
        let id = s(&it, "id");
        let kind = s(&it, "kind");
        let thumb = m.thumbs.get(&id).cloned().flatten();
        let note = match kind.as_str() { "video" => "Videos play in the system's player.", "audio" => "Audio plays in the system's player.", "pdf" => "PDFs open in the system's viewer.", _ => "" };
        let mv = self.0.clone();
        div().flex().flex_col().w(T::WS_DIFF_W).h(T::WS_DIFF_H)
            .child(w::modal_head("lb-head", s(&it, "originalName"), false, |_, window, cx| w::close_dialog(window, cx)))
            .child(div().flex_1().min_h_0().flex().items_center().justify_center().bg(T::bg()).p(T::SP_12)
                .child(match thumb { Some(img) => gpui::img(img).max_w_full().max_h_full().object_fit(ObjectFit::Contain).into_any_element(), None => w::empty(if note.is_empty() { "Loading…" } else { note }).into_any_element() }))
            .child(w::toolbar().px(T::SP_16).py(T::SP_10).child(div().flex_1()).child(w::button("lb-download", "Download").icon(IconName::Download).on_click(move |_, _, cx| { let id = id.clone(); mv.update(cx, |m, cx| m.get(&id, cx)); })))
            .into_any_element()
    }
}

impl Render for MediaView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let filt = |id: &'static str, label: &'static str, on: bool| w::toggle(id, label, on);
        let has_project = !crundi_shell::project(cx).is_empty();
        // The bar wraps in a narrow pane (the web's flex-wrap): the search
        // keeps its width instead of collapsing (F86).
        let bar = w::toolbar().flex_wrap()
            .child(w::primary_icon("media-upload", IconName::Upload, "Upload").on_click(cx.listener(|m, _, _, cx| m.upload(cx))))
            .child(div().flex_none().w(T::WS_MEDIA_SEARCH).child(w::input(&self.search)))
            .child(filt("mk-all", "All", self.kind == "all").on_click(cx.listener(|m, _, _, cx| m.set_kind("all", cx))))
            .child(filt("mk-kanban", "Kanban", self.kind == "kanban").on_click(cx.listener(|m, _, _, cx| m.set_kind("kanban", cx))))
            .child(filt("mk-mindmap", "Mindmap", self.kind == "mindmap").on_click(cx.listener(|m, _, _, cx| m.set_kind("mindmap", cx))))
            .child(filt("mk-unlinked", "Unlinked", self.kind == "unlinked").on_click(cx.listener(|m, _, _, cx| m.set_kind("unlinked", cx))))
            .child(div().flex_1())
            .child(w::disabled(filt("ms-project", "This project", self.scope == "project"), !has_project).on_click(cx.listener(|m, _, _, cx| m.set_scope("project", cx))))
            .child(filt("ms-all", "All projects", self.scope == "all").on_click(cx.listener(|m, _, _, cx| m.set_scope("all", cx))));
        let shown = self.shown();
        let body: AnyElement = match self.state.as_str() {
            "loading" => w::loading("Loading…").into_any_element(),
            "error" => w::error_state("media-retry", self.err.clone(), cx.listener(|m, _, _, cx| { m.state = "loading".into(); m.load(cx); })).into_any_element(),
            _ if self.scope == "project" && !has_project => w::empty("Select a project, or switch to All projects.").into_any_element(),
            _ if self.items.is_empty() => w::empty("No media here yet. Upload files, or attach them to a Kanban task/subtask or a Mindmap idea.").into_any_element(),
            _ if shown.is_empty() => w::empty(format!("No media matches \u{201c}{}\u{201d}.", self.q)).into_any_element(),
            _ => {
                let mut grid = div().flex().flex_wrap().gap(T::SP_12).p(T::SP_12);
                for (i, it) in shown.iter().enumerate() { grid = grid.child(self.card(it, i, cx)); }
                div().id("media-grid").flex_1().min_h_0().overflow_y_scroll().child(grid).into_any_element()
            }
        };
        div().flex().flex_col().size_full().bg(T::bg()).child(bar).child(body)
    }
}

pub fn ensure(window: &mut Window, cx: &mut App) -> Entity<MediaView> {
    if let Some(v) = view(cx) { return v; }
    let v = cx.new(|cx| MediaView::new(window, cx));
    cx.set_global(MediaHandle(v.clone()));
    v.update(cx, |m, cx| m.load(cx));
    v
}

#[cfg(test)]
mod tests {
    use super::{link_word, list_url, matches};
    use serde_json::json;

    #[test]
    fn urls_and_filters() {
        assert_eq!(list_url("project", "all", "demo"), "/api/media/list?project=demo&includeGeneral=1");
        assert_eq!(list_url("all", "kanban", "demo"), "/api/media/list?kind=kanban");
        let it = json!({ "originalName": "logo.png", "ext": ".png", "kind": "image", "link": { "type": "node" } });
        assert!(matches(&it, "LOGO") && !matches(&it, "zip"));
        assert_eq!(link_word(&it), "Idea");
    }
}
