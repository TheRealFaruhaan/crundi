//! Media (phase 3.5; the web's renderMediaBrowser and the media* actions,
//! the Slint client's src/workspace/media.rs): Upload, the search, All /
//! Kanban / Mindmap / Unlinked, This project / All projects; the cards
//! (the kind's badge, the thumbnail or kind and extension, the name, the
//! project and the link to the source, View or Get, Copy path, Jump to
//! source, Delete); a card drags onto a chat or terminal (its path). The
//! lightbox is a dark layer over the whole window (the web's
//! #media-lightbox): the name, Download and Close, the picture, or the PDF,
//! video or audio playing (frames.rs: the Desktop area's browser).
//!
//! The cards on show are filtered once per change (a load, a key in the
//! search), never while drawing; the grid draws only the rows of cards on
//! screen (`uniform_list` of rows, as many cards a row as its width holds).

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
    lightbox: Entity<Lightbox>,
    search: Entity<w::InputState>,
    q: String,
    /// The cards on show (the search applied), and how many fit a row.
    shown: Vec<Value>,
    cols: usize,
    /// The grid's width (cards share it: minmax(150px, 1fr)).
    width: f32,
    project: String,
    grid: UniformListScrollHandle,
    /// Each card's place on screen as last painted (by id), for the test driver.
    card_at: std::rc::Rc<std::cell::RefCell<HashMap<String, Bounds<Pixels>>>>,
    _sub: Subscription,
}

pub struct MediaHandle(pub Entity<MediaView>);
impl Global for MediaHandle {}
pub fn view(cx: &App) -> Option<Entity<MediaView>> { cx.try_global::<MediaHandle>().map(|h| h.0.clone()) }

impl MediaView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| w::InputState::new(window, cx).placeholder("Search media…"));
        let _sub = cx.subscribe_in(&search, window, |m, st, ev: &w::InputEvent, _, cx| if let w::InputEvent::Change = ev { let q = st.read(cx).value().to_string(); if q != m.q { m.q = q; m.derive(cx); } });
        let lightbox = lightbox(cx).expect("media::init first");
        MediaView { kind: "all".into(), scope: "project".into(), items: vec![], state: "loading".into(), err: String::new(), thumbs: HashMap::new(), lb: None, lightbox, search, q: String::new(), shown: vec![], cols: 1, width: 0., project: crundi_shell::project(cx), grid: UniformListScrollHandle::new(), card_at: Default::default(), _sub }
    }

    pub fn shown(&self) -> Vec<Value> { self.items.iter().filter(|it| matches(it, &self.q)).cloned().collect() }

    /// The cards on show again (once per change).
    fn derive(&mut self, cx: &mut Context<Self>) {
        self.shown = self.shown();
        self.project = crundi_shell::project(cx);
        cx.notify();
    }

    pub fn set_search(&mut self, q: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.q = q.to_string();
        self.search.update(cx, |st, cx| st.set_value(q.to_string(), window, cx));
        self.derive(cx);
    }

    /// GET the list for the scope and kind; a failed load says so (Retry).
    pub fn load(&mut self, cx: &mut Context<Self>) {
        let project = crundi_shell::project(cx);
        if project.is_empty() && self.scope == "project" { self.scope = "all".into(); }
        self.project = project.clone();
        let path = list_url(&self.scope, &self.kind, &project);
        let me = cx.entity().downgrade();
        crundi_shell::call(cx, move |api| api.get(&path), move |r, cx| {
            let _ = me.update(cx, |m, cx| {
                match r {
                    Ok(v) if v.get("ok").is_some_and(truthy) => { m.items = v.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default(); m.state = "ok".into(); m.err.clear(); }
                    // A failed list is an empty one (fetchMedia), its reason kept for the driver.
                    Ok(v) => { m.items.clear(); m.state = "ok".into(); m.err = s(&v, "error"); }
                    Err(e) => { m.items.clear(); m.state = "ok".into(); m.err = e.to_string(); }
                }
                m.thumbs_wanted(cx);
                m.derive(cx);
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
            let format = match ext.trim_start_matches('.').to_ascii_lowercase().as_str() { "jpg" | "jpeg" => ImageFormat::Jpeg, "gif" => ImageFormat::Gif, "webp" => ImageFormat::Webp, "svg" => ImageFormat::Svg, "bmp" => ImageFormat::Bmp, "ico" => ImageFormat::Ico, _ => ImageFormat::Png };
            let me = cx.entity().downgrade();
            crundi_shell::run(cx, move || match api.get_bytes(&path) { Ok((200, b)) => Some(b), _ => None }, move |b, cx| {
                if let Some(b) = b { let _ = me.update(cx, |m, cx| {
                    let img = Arc::new(Image::from_bytes(format, b));
                    m.lightbox.update(cx, |l, cx| if l.id == id && l.img.is_none() { l.img = Some(img.clone()); cx.notify(); });
                    m.thumbs.insert(id, Some(img));
                    cx.notify();
                }); }
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

    /// View: a picture, PDF, video or audio in the lightbox (openMediaLightbox); anything else downloads.
    pub fn open(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(it) = self.item(id) else { return };
        if s(&it, "kind") == "other" { return self.get(id, cx); }
        if s(&it, "kind") == "image" && !self.thumbs.contains_key(id) { self.thumbs_wanted(cx); }
        let img = self.thumbs.get(id).cloned().flatten();
        let url = crundi_shell::api(cx).map(|a| format!("{}/api/media/raw/{}?token={}", a.base().trim_end_matches('/'), crate::enc(id), crate::enc(&a.access()))).unwrap_or_default();
        self.lb = Some(it.clone());
        let me = cx.entity().downgrade();
        self.lightbox.update(cx, |l, cx| { l.show(it, img, url, me); cx.notify(); });
        cx.notify();
    }

    /// The lightbox closed (Close, or a press on the dark around it).
    pub fn close_lightbox(&mut self, cx: &mut Context<Self>) {
        self.lb = None;
        self.lightbox.update(cx, |l, cx| { l.hide(); cx.notify(); });
        cx.notify();
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
        let _ = window;
        if self.lb.is_some() { self.close_lightbox(cx); }
        let p = s(&it, "project");
        let stores = crundi_shell::stores(cx);
        if !p.is_empty() && p != crundi_shell::project(cx) {
            if !stores.projects.read(cx).paths().iter().any(|(a, _)| *a == p) { return toast(cx, "Source project not available".into(), w::Toast::Error); }
            stores.projects.update(cx, |x, cx| x.pick(&p, cx));
        }
        let node = it.get("link").map(|l| s(l, "type") == "node").unwrap_or(false);
        stores.nav.update(cx, |n, cx| n.show_tab(if node { "mindmap" } else { "kanban" }, cx));
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
            "cards": self.shown.iter().map(|c| { let id = s(c, "id"); let bx = self.card_at.borrow().get(&id).map(|b| [f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height)]);
                json!({ "id": id, "name": s(c, "originalName"), "kind": s(c, "kind"), "box": bx,
                "thumb": self.thumbs.get(&id).is_some_and(|t| t.is_some()), "link": if c.get("link").is_none_or(|l| l.is_null()) { "none" } else if s(c, "linkStatus") == "deleted" { "deleted" } else { "ok" } }) }).collect::<Vec<_>>(),
            "lightbox": self.lb.as_ref().map(|i| s(i, "originalName")),
        })
    }

    /// A card (mediaCardHtml): the thumbnail with the kind's badge, the
    /// name, the project and the link to the source, the actions.
    fn card(&self, it: &Value, i: usize, cw: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let id = s(it, "id");
        let kind = s(it, "kind");
        let ext = { let e = s(it, "ext").trim_start_matches('.').to_string(); if e.is_empty() { kind.clone() } else { e } }.to_uppercase();
        let thumb = self.thumbs.get(&id).cloned().flatten();
        let linked = it.get("link").is_some_and(|l| !l.is_null());
        let deleted = s(it, "linkStatus") == "deleted";
        let path = s(it, "path");
        let glyph = kind_icon(&kind);
        let name = s(it, "originalName");
        let (a, b, c, d) = (id.clone(), id.clone(), id.clone(), id.clone());
        // The source (mediaLinkSubHtml): a link to it, or struck through once deleted.
        let word = link_word(it);
        let link: AnyElement = if !linked { div().child("Unlinked").into_any_element() }
            else if deleted { div().flex().items_center().gap(T::SP_2).line_through().child(w::icon(IconName::ExternalLink, T::TEXT_10)).child(format!("{word} (deleted)")).into_any_element() }
            else {
                let label = { let l = s(it, "linkLabel"); trunc(if l.is_empty() { &word } else { &l }, 22) };
                let j = id.clone();
                w::tip(div().id(("media-link", i)).flex().items_center().gap(T::SP_2).text_color(T::green()).cursor_pointer().hover(|d| d.underline())
                    .child(w::icon(IconName::ExternalLink, T::TEXT_10)).child(label)
                    .on_click(cx.listener(move |m, _, window, cx| m.jump(&j, window, cx))), format!("Jump to {}", word.to_lowercase())).into_any_element()
            };
        let project = s(it, "projectName");
        let thumb_box = div().id(("media-thumb", i)).relative().h(T::WS_MEDIA_THUMB).w_full().flex().items_center().justify_center().overflow_hidden().bg(T::bg_secondary()).cursor_pointer()
            .on_click(cx.listener(move |m, _, _, cx| m.open(&a, cx)))
            .child(match thumb {
                Some(img) => gpui::img(img).size_full().object_fit(ObjectFit::Cover).into_any_element(),
                None => div().flex().flex_col().items_center().gap(T::SP_6).text_color(T::text_muted())
                    .child(w::icon(glyph.clone(), T::ICON_XL)).child(div().text_size(T::TEXT_10).child(ext)).into_any_element(),
            })
            // The kind's badge (.media-kindbadge), top left.
            .child(div().absolute().top(T::SP_6).left(T::SP_6).px(T::SP_4).py(T::SP_2).rounded(T::SP_6).bg(T::ws_ml_scrim().opacity(T::WS_MEDIA_BADGE_O)).text_color(T::white()).child(w::icon(glyph, T::TEXT_13)));
        let card = w::card().id(("media-card", i)).w(cw).overflow_hidden().bg(T::bg_card());
        let card = if path.is_empty() { card } else { w::tip(w::drag_ref(card.cursor_grab(), "media", &path, name.clone()), "Drag onto a terminal to insert this path") };
        let (at, k) = (self.card_at.clone(), id.clone());
        card.relative().child(thumb_box)
            .child(div().flex().flex_col().gap(T::SP_2).px(T::SP_8).py(T::SP_6)
                .child(w::tip(div().id(("media-name", i)).text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text()).line_clamp(2).child(name.clone()), name))
                .child(div().flex().flex_wrap().items_center().gap(T::SP_6).text_size(T::TEXT_10).text_color(T::text_muted())
                    .when(!project.is_empty(), |d| d.child(trunc(&project, 16)))
                    .child(link)))
            .child(div().flex().gap(T::SP_4).px(T::SP_8).pb(T::SP_8)
                .child(div().flex_1().child(if kind == "other" {
                    w::inner(w::icon_label_button(("media-view", i), IconName::Download, "Get").tooltip("Download")).w_full().on_click(cx.listener(move |m, _, _, cx| m.open(&b, cx)))
                } else {
                    w::inner(w::icon_label_button(("media-view", i), IconName::Image, "View").tooltip("Preview")).w_full().on_click(cx.listener(move |m, _, _, cx| m.open(&b, cx)))
                }))
                .when(!path.is_empty(), |d0| d0.child(w::icon_button(("media-copy", i), IconName::Copy, "Copy file path").on_click(cx.listener(move |m, _, _, cx| m.copy(&c, cx)))))
                .when(linked, |d0| { let j = id.clone(); d0.child(w::disabled(w::icon_button(("media-jump", i), IconName::ExternalLink, "Jump to source"), deleted).on_click(cx.listener(move |m, _, window, cx| m.jump(&j, window, cx)))) })
                .child(w::icon_button(("media-del", i), IconName::Trash, "Delete").on_click(cx.listener(move |m, _, window, cx| m.delete(&d, window, cx)))))
            .child(canvas(move |b, _, _| { at.borrow_mut().insert(k.clone(), b); }, |_, _, _, _| {}).absolute().top_0().left_0().size_full())
            .into_any_element()
    }
}

fn kind_icon(kind: &str) -> IconName { match kind { "video" => IconName::Film, "audio" => IconName::Music, "pdf" => IconName::FileText, "image" => IconName::Image, _ => IconName::File } }

/// A label cut to n characters with an ellipsis (mtrunc).
fn trunc(t: &str, n: usize) -> String { if t.chars().count() > n { format!("{}…", t.chars().take(n - 1).collect::<String>()) } else { t.to_string() } }

/// The lightbox (#media-lightbox): its own layer over the window, drawn by
/// the shell (register_overlay), so opening it redraws nothing else.
pub struct Lightbox {
    pub id: String,
    item: Option<Value>,
    img: Option<Arc<Image>>,
    url: String,
    media: WeakEntity<MediaView>,
}

impl Lightbox {
    fn show(&mut self, it: Value, img: Option<Arc<Image>>, url: String, media: WeakEntity<MediaView>) {
        self.id = s(&it, "id");
        (self.item, self.img, self.url, self.media) = (Some(it), img, url, media);
    }
    fn hide(&mut self) { self.id.clear(); self.item = None; self.img = None; self.url.clear(); }
}

impl Render for Lightbox {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("lightbox");
        let Some(it) = self.item.clone() else { return div().into_any_element() };
        let id = s(&it, "id");
        let kind = s(&it, "kind");
        let vs = window.viewport_size();
        let (mw, mh) = (vs.width * T::WS_ML_W, vs.height * T::WS_ML_H);
        let media = self.media.clone();
        let close = move |cx: &mut App| { let _ = media.update(cx, |m, cx| m.close_lightbox(cx)); };
        let (c1, c2) = (close.clone(), close);
        let stage: AnyElement = if kind == "image" {
            match &self.img {
                Some(img) => div().id("ml-stage").max_w(mw).max_h(mh).flex().items_center().justify_center().shadow_lg()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(gpui::img(img.clone()).max_w(mw).max_h(mh).object_fit(ObjectFit::Contain)).into_any_element(),
                None => div().into_any_element(),
            }
        } else {
            // A PDF, video or audio plays in the browser (iframe, <video controls autoplay>, <audio>).
            let h = if kind == "audio" { T::WS_ML_AUDIO_H } else { mh };
            div().id("ml-stage").relative().w(mw).h(h).rounded(T::SP_6).overflow_hidden().when(kind == "pdf", |d| d.bg(T::white()))
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(crate::frames::page(&format!("lb-{id}"), &self.url, window, cx)).into_any_element()
        };
        let mv = self.media.clone();
        div().id("media-lightbox").occlude().absolute().inset_0().flex().flex_col().items_center().justify_center().p(T::SP_24).bg(T::ws_ml_scrim())
            .on_click(move |_, _, cx| c1(cx))
            .child(div().absolute().top_0().left_0().w_full().flex().items_center().gap(T::SP_10).px(T::SP_16).py(T::SP_12).text_color(T::white())
                .child(div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).child(s(&it, "originalName")))
                .child(div().flex_1())
                .child(w::scrim_button("ml-download", "Download").on_click(move |_, _, cx| { cx.stop_propagation(); let id = id.clone(); let _ = mv.update(cx, |m, cx| m.get(&id, cx)); }))
                .child(w::scrim_button("ml-close", "Close").on_click(move |_, _, cx| { cx.stop_propagation(); c2(cx) })))
            .child(stage)
            .into_any_element()
    }
}

struct LightboxHandle(Entity<Lightbox>);
impl Global for LightboxHandle {}
fn lightbox(cx: &App) -> Option<Entity<Lightbox>> { cx.try_global::<LightboxHandle>().map(|h| h.0.clone()) }

/// The lightbox's layer (at the area's init: the shell takes its overlays then).
pub fn init(cx: &mut App) {
    let l = cx.new(|_| Lightbox { id: String::new(), item: None, img: None, url: String::new(), media: WeakEntity::new_invalid() });
    cx.set_global(LightboxHandle(l.clone()));
    crundi_shell::register_overlay(cx, l.into());
}

impl Render for MediaView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crundi_core::driver::rendered("media");
        let chip = |id: &'static str, label: &'static str, on: bool| w::toggle(id, label, on);
        let has_project = !self.project.is_empty();
        // The bar wraps in a narrow pane (the web's flex-wrap): the search
        // keeps its width instead of collapsing (F86).
        let bar = w::toolbar().flex_wrap().gap(T::SP_8).px(T::SP_12).py(T::SP_10)
            .child(w::primary_icon("media-upload", IconName::Upload, "Upload").on_click(cx.listener(|m, _, _, cx| m.upload(cx))))
            .child(div().flex_none().w(T::WS_MEDIA_SEARCH).child(w::input(&self.search).font_family(T::MONO)))
            .child(chip("mk-all", "All", self.kind == "all").on_click(cx.listener(|m, _, _, cx| m.set_kind("all", cx))))
            .child(chip("mk-kanban", "Kanban", self.kind == "kanban").icon(IconName::Kanban).on_click(cx.listener(|m, _, _, cx| m.set_kind("kanban", cx))))
            .child(chip("mk-mindmap", "Mindmap", self.kind == "mindmap").icon(IconName::Network).on_click(cx.listener(|m, _, _, cx| m.set_kind("mindmap", cx))))
            .child(chip("mk-unlinked", "Unlinked", self.kind == "unlinked").on_click(cx.listener(|m, _, _, cx| m.set_kind("unlinked", cx))))
            .child(div().flex_1())
            .when(has_project, |d| d.child(chip("ms-project", "This project", self.scope == "project").on_click(cx.listener(|m, _, _, cx| m.set_scope("project", cx)))))
            .child(chip("ms-all", "All projects", self.scope == "all").on_click(cx.listener(|m, _, _, cx| m.set_scope("all", cx))));
        let body: AnyElement = match self.state.as_str() {
            // Before the first answer: nothing yet, as the web's.
            "loading" => div().into_any_element(),
            _ if self.scope == "project" && !has_project => media_empty(div().flex().justify_center().gap(T::SP_4).child("Select a project, or switch to").child(div().font_weight(T::W_BOLD).child("All projects")).child(".")).into_any_element(),
            _ if self.items.is_empty() => media_empty(div().child("No media here yet. Upload files, or attach them to a Kanban task/subtask or a Mindmap idea.")).into_any_element(),
            _ if self.shown.is_empty() => media_empty(div().child(format!("No media matches \u{201c}{}\u{201d}.", self.q))).into_any_element(),
            _ => {
                // The width decides how many cards a row holds and how wide
                // each is (minmax(150px, 1fr); measured; a change re-lays
                // the rows out once).
                let me = cx.entity();
                let measure = canvas(move |b, _, cx| {
                    let wd = f32::from(b.size.width);
                    if (me.read(cx).width - wd).abs() > 0.5 { let me = me.clone(); cx.defer(move |cx| me.update(cx, |m, cx| { m.width = wd; m.cols = cols_for(wd); cx.notify(); })); }
                }, |_, _, _, _| {}).absolute().top_0().left_0().size_full();
                let cols = self.cols.max(1);
                let cw = card_width(self.width, cols);
                let rows = self.shown.len().div_ceil(cols);
                let grid = uniform_list("media-grid", rows, cx.processor(move |m, range: std::ops::Range<usize>, _, cx| {
                    range.map(|r| {
                        let mut row = div().w_full().flex().gap(T::SP_12).px(T::SP_12).pt(T::SP_12);
                        for i in r * cols..((r + 1) * cols).min(m.shown.len()) { let it = m.shown[i].clone(); row = row.child(m.card(&it, i, px(cw), cx)); }
                        row.into_any_element()
                    }).collect::<Vec<_>>()
                })).track_scroll(&self.grid).size_full();
                // While a card is dragged near its edges, the grid scrolls.
                w::edge_scroll::<w::RefDrag>(div().id("media-body"), w::Scroller::Uniform(self.grid.clone()), false, true, cx.entity_id())
                    .relative().flex_1().min_h_0().child(measure).child(grid).into_any_element()
            }
        };
        div().flex().flex_col().size_full().bg(T::bg()).child(bar).child(body)
    }
}

/// The grid's lone line (.media-empty).
fn media_empty(d: Div) -> Div {
    div().w_full().flex().justify_center().px(T::SP_12).py(T::SP_40).text_size(T::TEXT_14).text_color(T::text_muted()).child(d)
}

/// A card's width in a row of `cols` across `width` (12 in from each side, 12 apart).
pub fn card_width(width: f32, cols: usize) -> f32 {
    let gap = f32::from(T::SP_12);
    ((width - 2. * gap - gap * (cols as f32 - 1.)) / cols as f32).max(f32::from(T::WS_MEDIA_CARD))
}

/// How many cards a row of this width holds (the web's wrapping grid:
/// cards of a fixed width, 12 apart, 12 in from each side).
pub fn cols_for(width: f32) -> usize {
    let (card, gap) = (f32::from(T::WS_MEDIA_CARD), f32::from(T::SP_12));
    // auto-fill, minmax(card, 1fr)
    (((width - 2. * gap + gap) / (card + gap)).floor() as usize).max(1)
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
    use super::{cols_for, link_word, list_url, matches};
    use serde_json::json;

    #[test]
    fn urls_and_filters() {
        assert_eq!(list_url("project", "all", "demo"), "/api/media/list?project=demo&includeGeneral=1");
        assert_eq!(list_url("all", "kanban", "demo"), "/api/media/list?kind=kanban");
        let it = json!({ "originalName": "logo.png", "ext": ".png", "kind": "image", "link": { "type": "node" } });
        assert!(matches(&it, "LOGO") && !matches(&it, "zip"));
        assert_eq!(link_word(&it), "Idea");
        assert_eq!(cols_for(100.), 1);
        assert_eq!(cols_for(12. + 150. + 12. + 150. + 12.), 2);
    }
}
