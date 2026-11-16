//! 2.16m: what a show_image / show_video / show_audio / show_file /
//! show_embed call showed, under its card (specs/log-rest.md section 4,
//! PLAN.md 2.16m), as the Slint client's MediaBlock / MediaView
//! (ui/chat.slint) and media_json / fetch_media / play_media /
//! download_file / viewer (src/app.rs). Drawn by tools.rs under a done call
//! (`under`). Every rule (names, sizes, words) is crundi_core::media's.
//!
//! Window-wide (a global, as the Slint client's app state): the decoded
//! pictures by "<chat>|<name>", the picture viewer, the download rows, and
//! the last thing opened by path (a played file).
use crate::LogView;
use crundi_core::{chat::Shown, media};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
pub struct State {}

/// A picture (or poster), as fetched: "loading" | "ready" | "gone" | "empty",
/// or "fetching" (a video or audio on its way to the system player).
#[derive(Clone, Default)]
pub struct Pic { pub state: String, pub img: Option<Arc<RenderImage>>, pub size: Option<(u32, u32)> }

/// The picture viewer: the gallery's keys, the one shown, zoom and pan.
pub struct Viewer { pub list: Vec<String>, pub at: usize, pub scale: f32, pub pan: (f32, f32), focus: Option<FocusHandle>, focused: bool, drag: Option<(Point<Pixels>, (f32, f32))> }

/// A download row (a show_file card's Download).
pub struct Dl { pub id: u32, pub chat: String, pub name: String, pub line: String, pub state: String, pub frac: f32, cancel: Arc<AtomicBool> }

#[derive(Default)]
pub struct Media {
    pub cache: HashMap<String, Pic>,
    pub viewer: Option<Viewer>,
    pub dls: Vec<Dl>,
    seq: u32,
    /// The last file opened by path, and the prose module's lastOpened when it was.
    opened: String,
    opened_over: String,
}
impl Global for Media {}

fn g(cx: &mut App) -> &mut Media { cx.default_global::<Media>() }
fn gr(cx: &App) -> Option<&Media> { cx.try_global::<Media>() }

/// The viewer's zoom: steps of 1.4, from 1 to 8; a double click 2.5.
const ZOOM_STEP: f32 = 1.4;
const ZOOM_MAX: f32 = 8.0;
const ZOOM_DOUBLE: f32 = 2.5;

/// The download rows leave by themselves after a while (ms).
const LINGER_DONE: u64 = 4500;
const LINGER_CANCEL: u64 = 1200;
const LINGER_FAIL: u64 = 7000;

/// This feature's fields in the chat's props (none: "media" is in each tool row, `media_json`).
pub fn props(_v: &LogView, _cx: &App, _o: &mut Map<String, Value>) {}

/// The window-wide fields (top level, the Slint client's names): media (the
/// focused chat's pictures: state and pixel size), viewer, downloads, and
/// lastOpened (the prose module's, or a file played since).
pub fn global_props(cx: &mut App, top: &mut Map<String, Value>) {
    let chat = crate::focused_chat(cx);
    let prose = crate::prose::global_props(cx).get("lastOpened").cloned().unwrap_or(json!(""));
    let m = g(cx);
    let pre = format!("{chat}|");
    let media: Map<String, Value> = m.cache.iter().filter(|(k, _)| k.starts_with(&pre)).map(|(k, p)| {
        (k[pre.len()..].to_string(), json!({ "state": p.state, "size": p.size.map(|(w, h)| json!([w, h])).unwrap_or(Value::Null) }))
    }).collect();
    let viewer = m.viewer.as_ref().map(|v| json!({ "at": v.at, "count": v.list.len(), "key": v.list[v.at], "ready": m.cache.get(&v.list[v.at]).is_some_and(|p| p.state == "ready") })).unwrap_or(Value::Null);
    let dls: Vec<Value> = m.dls.iter().map(|d| json!({ "name": d.name, "line": d.line, "state": d.state })).collect();
    let opened = if !m.opened.is_empty() && prose.as_str() == Some(m.opened_over.as_str()) { json!(m.opened) } else { prose };
    top.insert("media".into(), Value::Object(media));
    top.insert("viewer".into(), viewer);
    top.insert("downloads".into(), Value::Array(dls));
    top.insert("lastOpened".into(), opened);
}

/// This feature's driver actions (the Slint client's names): view <key>,
/// viewer-step <±1>, viewer-close, play <name> <filename>, download <id>
/// <filename>, open-frame <name> <url>.
pub fn act(name: &str, args: &[String], v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> Option<Value> {
    let args = args.to_vec();
    let a = move |i: usize| args.get(i).cloned().unwrap_or_default();
    let chat = v.chat(cx);
    if !matches!(name, "view" | "viewer-step" | "viewer-close" | "play" | "download" | "open-frame") { return None; }
    // (After this update of the log: they read and redraw the chat's log.)
    let _ = (window, v);
    let name = name.to_string();
    cx.defer(move |cx| match name.as_str() {
        "view" => view_picture(&a(0), cx),
        "viewer-step" => viewer_step(a(0).parse().unwrap_or(1), cx),
        "viewer-close" => viewer_close(cx),
        "play" => play(&chat, &a(0), &a(1), cx),
        "download" => download(&chat, &a(0), &a(1), cx),
        _ => open_frame(&chat, &a(0), &a(1), cx),
    });
    Some(json!({ "ok": true }))
}

// ─── The items of a call, as drawn ───

/// What a done show_* call showed (None: not one, running or failed).
fn items_of(t: &Shown) -> Option<(Vec<media::Item>, String)> {
    if t.status == "running" || t.error { return None; }
    let res = t.raw.as_str()?;
    Some(media::items(res))
}

/// Several pictures, videos or frames: side by side, 220 tall.
fn several(items: &[media::Item]) -> bool { items.iter().filter(|i| matches!(i.kind.as_str(), "image" | "video" | "embed")).count() > 1 }

/// An item's box (logical px): a picture's or video's, a frame's; none for files and audio.
pub fn box_of(i: &media::Item, several: bool) -> (f64, f64) {
    match i.kind.as_str() { "embed" => media::frame_size(i), "image" | "video" => media::box_size(i.w, i.h, several), _ => (0.0, 0.0) }
}

/// The stored name whose picture the item's box shows (a video's poster; a
/// stored frame's HTML is asked for too: a 404 is "gone"), or "".
fn pic_of(i: &media::Item) -> &str {
    match i.kind.as_str() { "image" => &i.name, "video" => &i.poster, "embed" => &i.name, _ => "" }
}

/// The item's caption and Open link (a frame's), as the web writes them.
fn caption_of(i: &media::Item) -> (String, String) {
    if i.kind == "embed" { media::frame_caption(i) } else { (media::item_caption(i), String::new()) }
}

/// A file's line: "12 B · The export".
pub fn file_line(i: &media::Item) -> String {
    let sz = media::fmt_bytes(i.size);
    if i.caption.is_empty() { sz } else { format!("{sz} · {}", i.caption) }
}

/// A show_* card's media as drawn, for the driver (the Slint client's
/// media_json: the call's caption; each item's kind, name, box, state,
/// caption, link and line). Null when the call showed nothing.
pub fn media_json(v: &LogView, t: &Shown, cx: &App) -> Value {
    let Some(res) = t.raw.as_str() else { return Value::Null };
    let (items, cap) = media::items(res);
    let chat = v.chat(cx);
    let sev = several(&items);
    let cache = gr(cx).map(|m| &m.cache);
    json!({ "caption": cap, "items": items.iter().map(|i| {
        let (w, h) = box_of(i, sev);
        let pic = if i.kind == "video" { &i.poster } else { &i.name };
        let state = cache.and_then(|c| c.get(&format!("{chat}|{pic}"))).map(|p| p.state.clone()).unwrap_or_default();
        let (cap, link) = caption_of(i);
        json!({ "kind": i.kind, "name": i.name, "w": w, "h": h, "state": state, "caption": cap, "link": link, "line": file_line(i) })
    }).collect::<Vec<_>>() })
}

// ─── Pictures: fetched and decoded off the UI thread ───

/// The picture for a key, asked for the first time it is drawn.
fn pic(key: &str, cx: &mut App) -> Pic {
    if let Some(p) = g(cx).cache.get(key) { return p.clone(); }
    g(cx).cache.insert(key.to_string(), Pic { state: "loading".into(), ..Default::default() });
    fetch(key.to_string(), cx);
    Pic { state: "loading".into(), ..Default::default() }
}

/// Decode a picture (PNG, JPEG, WebP, GIF: its first frame) into GPUI's BGRA image.
pub fn decode(bytes: &[u8]) -> Option<(Arc<RenderImage>, u32, u32)> {
    let mut rgba = image::load_from_memory(bytes).ok()?.to_rgba8();
    for p in rgba.pixels_mut() { p.0.swap(0, 2); }
    let (w, h) = rgba.dimensions();
    Some((Arc::new(RenderImage::new([image::Frame::new(rgba)])), w, h))
}

/// Ready, gone (404 / 410), empty (would not decode); a network failure is
/// forgotten, so it is asked for again the next time it is drawn.
fn fetch(key: String, cx: &mut App) {
    let Some(api) = crundi_shell::api(cx) else { g(cx).cache.remove(&key); return };
    let Some((chat, name)) = key.split_once('|').map(|(c, n)| (c.to_string(), n.to_string())) else { return };
    let path = format!("/api/chat-media?session={chat}&name={name}");
    crundi_shell::run(cx, move || match api.get_bytes(&path) {
        Ok((200, _)) if path.ends_with(".embed") => ("ready", None),
        Ok((200, bytes)) => match decode(&bytes) { Some(d) => ("ready", Some(d)), None => ("empty", None) },
        Ok((404, _)) | Ok((410, _)) => ("gone", None),
        _ => ("retry", None),
    }, move |(st, d), cx| {
        if st == "retry" { g(cx).cache.remove(&key); } else {
            g(cx).cache.insert(key.clone(), Pic { state: st.into(), size: d.as_ref().map(|x| (x.1, x.2)), img: d.map(|x| x.0) });
        }
        redraw(&chat, cx);
    });
}

/// The chat's log draws again (its pictures, the viewer, the downloads).
fn redraw(chat: &str, cx: &mut App) {
    if let Some(v) = crate::view_of(cx, chat) { v.update(cx, |_, cx| cx.notify()); }
}

// ─── Under the card ───

/// What a done show_* call showed, under its card (None: nothing). A
/// widget check's screenshots row comes after (2.20, cards.rs).
pub fn under(v: &mut LogView, t: &Shown, window: &mut Window, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let shots = crate::cards::shots_row(v, t, window, cx);
    let block = items_of(t).filter(|(items, _)| !items.is_empty()).map(|(items, cap)| block(v, t, &items, &cap, window, cx));
    match (block, shots) {
        (None, None) => None,
        (b, s) => Some(div().flex().flex_col().children(b).children(s).into_any_element()),
    }
}

/// The media under a call: one item, a row (8 apart, scrolling sideways), or
/// files and audio stacked; the call's caption under several.
fn block(v: &mut LogView, t: &Shown, items: &[media::Item], cap: &str, window: &mut Window, cx: &mut Context<LogView>) -> AnyElement {
    let chat = v.chat(cx);
    let sev = several(items);
    let stack = items.iter().all(|i| matches!(i.kind.as_str(), "file" | "audio"));
    let one = items.len() == 1;
    let els: Vec<AnyElement> = items.iter().enumerate().map(|(n, i)| item(v, &chat, &format!("{}-{n}", t.id), i, sev, one, window, cx)).collect();
    let body = if stack {
        div().flex().flex_col().gap(T::SP_8).children(els).into_any_element()
    } else {
        div().id(SharedString::from(format!("mrow-{}", t.id))).flex().gap(T::SP_8).items_start().overflow_x_scroll().children(els).into_any_element()
    };
    div().flex().flex_col().gap(T::SP_4).pt(T::SP_6).pb(T::SP_2)
        .child(body)
        .when((sev || items.is_empty()) && !cap.is_empty(), |d| d.child(caption(cap)))
        .into_any_element()
}

fn caption(text: &str) -> Div {
    div().max_w(T::DIALOG_W_LG - T::SP_20).text_size(T::TEXT_12).line_height(rems(1.35)).text_color(T::text_secondary()).child(text.to_string())
}

/// One medium: a picture (a click: the viewer), a video (its poster and the
/// play button: the system's player), an audio pill, a file card
/// (Download), a frame (its live page, caption and Open).
#[allow(clippy::too_many_arguments)]
fn item(v: &mut LogView, chat: &str, id: &str, i: &media::Item, sev: bool, one: bool, window: &mut Window, cx: &mut Context<LogView>) -> AnyElement {
    let (bw, bh) = box_of(i, sev);
    let p = { let n = pic_of(i); if n.is_empty() { Pic { state: "ready".into(), ..Default::default() } } else { pic(&format!("{chat}|{n}"), cx) } };
    let (cap, link) = caption_of(i);
    let fetching = gr(cx).and_then(|m| m.cache.get(&format!("{chat}|{}", i.name))).is_some_and(|p| p.state == "fetching");
    let kind = i.kind.as_str();
    let inner: AnyElement = match kind {
        "image" => {
            let key = format!("{chat}|{}", i.name);
            media_box(id, &p, bw, bh, one, false, "This image is no longer stored.")
                .cursor_pointer()
                .on_click(move |_, _, cx| view_picture(&key, cx))
                .into_any_element()
        }
        "video" => {
            let (c, n, f) = (chat.to_string(), i.name.clone(), i.filename.clone());
            media_box(id, &p, bw, bh, one, false, "This image is no longer stored.")
                .child(div().absolute().inset_0().flex().items_center().justify_center()
                    .child(if fetching { w::spinner(T::SP_48).into_any_element() } else {
                        w::icon_button(SharedString::from(format!("play-{id}")), IconName::Play, "Play").rounded_full().bg(T::scrim()).text_color(T::white())
                            .on_click(move |_, _, cx| play(&c, &n, &f, cx)).into_any_element()
                    }))
                .into_any_element()
        }
        "audio" => {
            let (c, n, f) = (chat.to_string(), i.name.clone(), i.filename.clone());
            let dur = if i.duration > 0.0 { media::fmt_duration(i.duration) } else { "0:00".into() };
            div().w(T::DIALOG_W_LG - T::SP_20).max_w_full().h(T::SP_48 + T::SP_2).rounded(T::R_PILL).bg(T::bg_card()).border_1().border_color(T::border())
                .flex().items_center().gap(T::SP_10).pl(T::SP_6)
                .child(if fetching { w::spinner(T::ICON_LG).into_any_element() } else {
                    w::icon_button(SharedString::from(format!("play-{id}")), IconName::Play, "Play").rounded_full().bg(T::accent()).text_color(T::white())
                        .on_click(move |_, _, cx| play(&c, &n, &f, cx)).into_any_element()
                })
                .child(div().font_family(T::MONO).text_size(T::TEXT_12).text_color(T::text_secondary()).child(format!("0:00 / {dur}")))
                .into_any_element()
        }
        "file" => {
            let (c, fid, f) = (chat.to_string(), i.id.clone(), i.filename.clone());
            div().w(T::DIALOG_W_LG).max_w_full().flex().items_center().gap(T::SP_10).px(T::SP_10).py(T::SP_8)
                .rounded(T::R_CARD).bg(T::bg_card()).border_1().border_color(T::border())
                .child(div().size(T::SP_32 + T::SP_2).flex_none().flex().items_center().justify_center().rounded(T::R_FIELD).bg(T::bg()).border_1().border_color(T::border())
                    .text_color(T::text_secondary()).child(w::icon(IconName::File, T::ICON_MD)))
                .child(div().flex_1().min_w_0().flex().flex_col().gap(T::SP_2)
                    .child(div().text_size(T::TEXT_13).font_weight(T::W_SEMIBOLD).text_color(T::text()).truncate().child(i.filename.clone()))
                    .child(div().text_size(T::TEXT_12).text_color(T::text_secondary()).truncate().child(file_line(i))))
                .child(w::button(SharedString::from(format!("dl-{id}")), "Download").icon(IconName::Download)
                    .on_click(move |_, _, cx| download(&c, &fid, &f, cx)))
                .into_any_element()
        }
        "embed" => {
            let fixed = !(i.w > 0.0 && i.h > 0.0);
            let target = if !i.name.is_empty() { format!("stored:{}", i.name) } else if i.src.starts_with("https://") || media::loopback(&i.src) { i.src.clone() } else { String::new() };
            let fkey = format!("{chat}|{target}");
            let inside = (p.state == "ready" && !target.is_empty()).then(|| crate::frames::view(v, &fkey, chat, i, window, cx));
            media_box(id, &p, bw, bh, one, fixed, "This embed is no longer stored.").children(inside).into_any_element()
        }
        _ => div().into_any_element(),
    };
    let url = if i.url.is_empty() { i.src.clone() } else { i.url.clone() };
    let cap_row = (kind != "file" && (!cap.is_empty() || !link.is_empty())).then(|| {
        div().flex().flex_wrap().gap(T::SP_4).items_baseline()
            .child(caption(&format!("{cap}{}", if link.is_empty() { "" } else { " ·" })))
            .when(!link.is_empty(), |d| d.child(div().id(SharedString::from(format!("open-{id}"))).text_size(T::TEXT_12).text_color(T::accent_hover()).underline().cursor_pointer().child(link.clone())
                .on_click(move |_, _, cx| crate::prose::open_link(&url, cx))))
    });
    div().flex().flex_col().gap(T::SP_4).flex_none().when(one, |d| d.max_w_full()).child(inner).children(cap_row).into_any_element()
}

/// A picture's, a video's or a frame's box: its size known before anything
/// loads (never wider than the pane when alone; the aspect follows, a frame
/// with a fixed height keeps it), the picture fitted inside; loading: the
/// ring; gone: a dashed edge and the words.
fn media_box(id: &str, p: &Pic, bw: f64, bh: f64, one: bool, fixed_h: bool, gone: &str) -> Stateful<Div> {
    let (bw, bh) = (bw as f32, bh as f32);
    let mut b = div().id(SharedString::from(format!("mbox-{id}"))).relative().flex_none().w(px(bw)).rounded(T::R_FIELD).overflow_hidden();
    b = if one && !fixed_h { b.max_w_full().aspect_ratio(bw / bh.max(1.)) } else { b.h(px(bh)).when(one, |b| b.max_w_full()) };
    if p.state == "gone" {
        return b.border_1().border_dashed().border_color(T::border()).flex().items_center().justify_center()
            .child(div().text_size(T::TEXT_12).text_color(T::text_muted()).child(gone.to_string()));
    }
    b = b.bg(if gone.contains("embed") { T::frame_bg() } else { T::bg_secondary() }).border_1().border_color(T::border());
    if p.state == "loading" {
        b = b.child(div().absolute().inset_0().flex().items_center().justify_center().child(w::spinner(T::SP_32 + T::SP_2)));
    }
    if let Some(img) = p.img.clone() {
        b = b.child(gpui::img(ImageSource::Render(img)).absolute().inset_0().size_full().object_fit(ObjectFit::Contain));
    }
    b
}

// ─── The viewer ───

/// The chat's pictures in order that loaded (the web's galleryOf: a gone box has none).
fn chat_pictures(chat: &str, cx: &App) -> Vec<String> {
    let Some(v) = crate::view_of(cx, chat) else { return vec![] };
    let cache = gr(cx).map(|m| &m.cache);
    v.read(cx).shown(cx).iter().filter(|e| e.kind == "tool" && e.status != "running" && !e.error)
        .filter_map(|e| e.raw.as_str().map(|r| media::items(r).0)).flatten()
        .filter(|i| i.kind == "image").map(|i| format!("{chat}|{}", i.name))
        .filter(|k| cache.and_then(|c| c.get(k)).is_some_and(|p| p.state == "ready")).collect()
}

/// A picture clicked: the viewer, on it, over the whole window.
pub fn view_picture(key: &str, cx: &mut App) {
    let chat = key.split_once('|').map(|x| x.0.to_string()).unwrap_or_default();
    // A widget check's screenshot is viewed on its own (2.20).
    let list = if key.contains("|@render:") { vec![key.to_string()] } else { chat_pictures(&chat, cx) };
    let Some(at) = list.iter().position(|k| k == key) else { return };
    g(cx).viewer = Some(Viewer { list, at, scale: 1.0, pan: (0.0, 0.0), focus: None, focused: false, drag: None });
    redraw(&chat, cx);
}

/// The viewer's step (wrapping round); the zoom resets.
pub fn viewer_step(by: i32, cx: &mut App) {
    let Some(v) = g(cx).viewer.as_mut() else { return };
    let n = v.list.len() as i32;
    v.at = (((v.at as i32 + by) % n + n) % n) as usize;
    v.scale = 1.0;
    v.pan = (0.0, 0.0);
    let chat = v.list[v.at].split('|').next().unwrap_or("").to_string();
    redraw(&chat, cx);
}

pub fn viewer_close(cx: &mut App) {
    let chat = g(cx).viewer.take().map(|v| v.list[v.at].split('|').next().unwrap_or("").to_string()).unwrap_or_default();
    redraw(&chat, cx);
}

/// The zoom, from 1 to 8 (back at 1: centred).
pub fn zoom_to(to: f32) -> f32 { to.clamp(1.0, ZOOM_MAX) }

fn zoom(cx: &mut App, f: impl Fn(f32) -> f32) {
    if let Some(v) = g(cx).viewer.as_mut() {
        v.scale = zoom_to(f(v.scale));
        if v.scale == 1.0 { v.pan = (0.0, 0.0); }
    }
}

/// Never further than the picture's edge plus 40.
pub fn clamp_pan(pan: (f32, f32), pic: (f32, f32), room: (f32, f32)) -> (f32, f32) {
    let m = f32::from(T::SP_40);
    let mx = ((pic.0 - room.0) / 2.0 + m).max(0.0);
    let my = ((pic.1 - room.1) / 2.0 + m).max(0.0);
    (pan.0.clamp(-mx, mx), pan.1.clamp(-my, my))
}

/// The window-level parts this chat's log draws over everything: the
/// viewer (when its picture is this chat's) and the download rows (theirs).
pub fn overlay(v: &mut LogView, window: &mut Window, cx: &mut Context<LogView>) -> Vec<AnyElement> {
    let chat = v.chat(cx);
    let mut out = vec![];
    if let Some(t) = tray(&chat, window, cx) { out.push(t); }
    if let Some(e) = viewer(&chat, window, cx) { out.push(e); }
    out
}

fn viewer(chat: &str, window: &mut Window, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let m = g(cx);
    let vw = m.viewer.as_ref()?;
    let key = vw.list[vw.at].clone();
    if !key.starts_with(&format!("{chat}|")) { return None; }
    if vw.focus.is_none() { let h = cx.focus_handle(); if let Some(vw) = g(cx).viewer.as_mut() { vw.focus = Some(h); } }
    let vw = g(cx).viewer.as_mut()?;
    let focus = vw.focus.clone()?;
    let first = !std::mem::replace(&mut vw.focused, true);
    if first { window.focus(&focus, cx); }
    let m = g(cx);
    let vw = m.viewer.as_ref()?;
    let (at, n, scale, pan) = (vw.at, vw.list.len(), vw.scale, vw.pan);
    let pic = m.cache.get(&key).cloned();
    if pic.is_none() { let _ = self::pic(&key, cx); }
    let room = window.viewport_size();
    let (rw, rh) = (f32::from(room.width), f32::from(room.height));
    let pad = f32::from(T::SP_16);
    let (iw, ih) = pic.as_ref().and_then(|p| p.size).map(|(w, h)| (w as f32, h as f32)).unwrap_or((1.0, 1.0));
    let fit = ((rw - 2.0 * pad) / iw).min((rh - 2.0 * pad) / ih).min(1.0);
    let (pw, ph) = (iw * fit * scale, ih * fit * scale);
    let picture = pic.and_then(|p| p.img).map(|img| {
        gpui::img(ImageSource::Render(img)).absolute().left(px((rw - pw) / 2.0 + pan.0)).top(px((rh - ph) / 2.0 + pan.1)).w(px(pw)).h(px(ph))
    });
    let bar = div().absolute().top(T::SP_10).right(T::SP_10).flex().gap(T::SP_2).p(T::SP_2).rounded(T::R_PILL).bg(T::bg_secondary()).border_1().border_color(T::border())
        .child(w::icon_button("vw-out", IconName::Minus, "Zoom out").on_click(|_, _, cx| zoom(cx, |s| s / ZOOM_STEP)))
        .child(w::ghost("vw-fit", format!("{}%", (scale * 100.0).round() as i32)).tooltip("Back to fit").on_click(|_, _, cx| zoom(cx, |_| 1.0)))
        .child(w::icon_button("vw-in", IconName::Plus, "Zoom in").on_click(|_, _, cx| zoom(cx, |s| s * ZOOM_STEP)))
        .child(w::icon_button("vw-x", IconName::X, "Close").on_click(|_, _, cx| viewer_close(cx)));
    let sides = (n > 1 && scale == 1.0).then(|| vec![
        div().absolute().left(T::SP_12).top(px(rh / 2.0)).child(w::icon_button("vw-prev", IconName::ChevronLeft, "Previous picture").rounded_full().bg(T::bg_secondary()).on_click(|_, _, cx| viewer_step(-1, cx))),
        div().absolute().right(T::SP_12).top(px(rh / 2.0)).child(w::icon_button("vw-next", IconName::ChevronRight, "Next picture").rounded_full().bg(T::bg_secondary()).on_click(|_, _, cx| viewer_step(1, cx))),
    ]).unwrap_or_default();
    let counter = (n > 1).then(|| div().absolute().top(T::SP_12 + T::SP_2).left(T::SP_12 + T::SP_2).px(T::SP_12).py(T::SP_4).rounded(T::R_PILL).bg(T::bg_secondary()).border_1().border_color(T::border())
        .font_family(T::MONO).text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text_secondary()).child(format!("{} / {n}", at + 1)));
    let el = div().id("viewer").track_focus(&focus).w(room.width).h(room.height).bg(T::backdrop()).occlude()
        .on_key_down(|e: &KeyDownEvent, _, cx| {
            let n = gr(cx).and_then(|m| m.viewer.as_ref()).map(|v| v.list.len()).unwrap_or(0);
            match e.keystroke.key.as_str() {
                "escape" => viewer_close(cx),
                "left" if n > 1 => viewer_step(-1, cx),
                "right" if n > 1 => viewer_step(1, cx),
                "+" | "=" => zoom(cx, |s| s * ZOOM_STEP),
                "-" => zoom(cx, |s| s / ZOOM_STEP),
                "0" => zoom(cx, |_| 1.0),
                _ => return,
            }
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, _, cx| {
            let inside = e.position.x >= px((rw - pw) / 2.0 + pan.0) && e.position.x <= px((rw + pw) / 2.0 + pan.0) && e.position.y >= px((rh - ph) / 2.0 + pan.1) && e.position.y <= px((rh + ph) / 2.0 + pan.1);
            if !inside { viewer_close(cx); return; }
            if e.click_count == 2 { zoom(cx, |s| if s > 1.0 { 1.0 } else { ZOOM_DOUBLE }); return; }
            if let Some(v) = g(cx).viewer.as_mut() { v.drag = Some((e.position, v.pan)); }
        })
        .on_mouse_up(MouseButton::Left, |_, _, cx| { if let Some(v) = g(cx).viewer.as_mut() { v.drag = None; } })
        .on_mouse_move(move |e: &MouseMoveEvent, _, cx| {
            let Some(v) = g(cx).viewer.as_mut() else { return };
            let Some((at, from)) = v.drag else { return };
            if v.scale <= 1.0 || e.pressed_button != Some(MouseButton::Left) { return; }
            let d = e.position - at;
            v.pan = clamp_pan((from.0 + f32::from(d.x), from.1 + f32::from(d.y)), (pw, ph), (rw, rh));
            let chat = v.list[v.at].split('|').next().unwrap_or("").to_string();
            redraw(&chat, cx);
        })
        .on_scroll_wheel(move |e: &ScrollWheelEvent, window, cx| {
            let d = e.delta.pixel_delta(window.line_height());
            if e.modifiers.control || e.modifiers.platform { zoom(cx, |s| if f32::from(d.y) > 0.0 { s * ZOOM_STEP } else { s / ZOOM_STEP }); }
            else if let Some(v) = g(cx).viewer.as_mut() { if v.scale > 1.0 { v.pan = clamp_pan((v.pan.0 + f32::from(d.x), v.pan.1 + f32::from(d.y)), (pw, ph), (rw, rh)); } }
        })
        .children(picture).children(counter).child(bar).children(sides);
    Some(deferred(anchored().position(point(px(0.), px(0.))).child(el)).with_priority(2).into_any_element())
}

// ─── Play: the system's player ───

/// Play a video or audio: fetched (with the sign-in, never a URL with the
/// token) to the temp folder under its stored name (kept if it is already
/// there), then the system's player opens the file. (Under the test driver
/// only the path is recorded.)
pub fn play(chat: &str, name: &str, filename: &str, cx: &mut App) {
    let key = format!("{chat}|{name}");
    if g(cx).cache.get(&key).is_some_and(|p| p.state == "fetching") { return; }
    let Some(api) = crundi_shell::api(cx) else { return };
    g(cx).cache.insert(key.clone(), Pic { state: "fetching".into(), ..Default::default() });
    redraw(chat, cx);
    let path = std::env::temp_dir().join(name);
    let url = format!("/api/chat-media?session={chat}&name={name}");
    let (chat, label) = (chat.to_string(), if filename.is_empty() { name.to_string() } else { filename.to_string() });
    crundi_shell::run(cx, move || -> Result<std::path::PathBuf, String> {
        if path.metadata().is_ok_and(|m| m.len() > 0) { return Ok(path); }
        stream_to(&api, &url, &path, &mut |_, _| {}, &AtomicBool::new(false)).map(|_| path)
    }, move |r, cx| {
        g(cx).cache.remove(&key);
        redraw(&chat, cx);
        match r {
            Ok(p) => opened_path(p, cx),
            Err(e) => toast(cx, &if e.is_empty() { format!("Could not play {label}") } else { e }),
        }
    });
}

/// A file opened by path: recorded (the driver's lastOpened), then the system's app (not under the driver).
fn opened_path(p: std::path::PathBuf, cx: &mut App) {
    let over = crate::prose::global_props(cx).get("lastOpened").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let m = g(cx);
    m.opened = p.to_string_lossy().to_string();
    m.opened_over = over;
    if std::env::var_os("CRUNDI_TEST_PORT").is_none() { cx.open_with_system(&p); }
}

/// A frame's Open: its page in the system browser; stored HTML (no url)
/// through a temp file.
pub fn open_frame(chat: &str, name: &str, url: &str, cx: &mut App) {
    if !url.is_empty() { crate::prose::open_link(url, cx); return; }
    let Some(api) = crundi_shell::api(cx) else { return };
    let path = std::env::temp_dir().join(format!("{}.html", name.trim_end_matches(".embed")));
    let q = format!("/api/chat-media?session={chat}&name={name}");
    crundi_shell::run(cx, move || api.get_bytes(&q).ok().filter(|r| r.0 == 200).and_then(|r| std::fs::write(&path, r.1).ok().map(|_| path)), |p, cx| match p {
        Some(p) => opened_path(p, cx),
        None => toast(cx, "This embed is no longer stored."),
    });
}

fn toast(cx: &mut App, text: &str) {
    let Some(win) = cx.active_window().or_else(|| cx.windows().first().copied()) else { return };
    let t = text.to_string();
    let _ = win.update(cx, move |_, window, cx| w::toast(window, cx, t, w::Toast::Error));
}

// ─── Downloads ───

/// Stream a GET to `<path>.part`, renamed to the path when complete:
/// the bytes, or the server's error ("\0cancel" when cancelled).
fn stream_to(api: &crundi_core::api::Api, url: &str, path: &std::path::Path, prog: &mut dyn FnMut(u64, u64), cancel: &AtomicBool) -> Result<u64, String> {
    use std::io::Write;
    let part = { let mut p = path.as_os_str().to_owned(); p.push(".part"); std::path::PathBuf::from(p) };
    let res = (|| {
        let mut f = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let (mut got, mut werr) = (0u64, None);
        let r = api.get_stream(url, &mut |b, total| {
            if cancel.load(Ordering::Relaxed) { return false; }
            if let Err(e) = f.write_all(b) { werr = Some(e.to_string()); return false; }
            got += b.len() as u64;
            prog(got, total);
            true
        }).map_err(|e| e.to_string())?;
        drop(f);
        if let Some(e) = werr { return Err(e); }
        match r {
            (200, _) => { std::fs::rename(&part, path).map_err(|e| e.to_string())?; Ok(got) }
            (499, _) => Err("\u{0}cancel".into()),
            (st, text) => Err(serde_json::from_str::<Value>(&text).ok().and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string)).unwrap_or_else(|| format!("Download failed ({st})"))),
        }
    })();
    if res.is_err() { let _ = std::fs::remove_file(&part); }
    res
}

/// Download: where to save first (the driver's answer, else the system's
/// dialog, the Downloads folder, the name filled in; cancel: nothing), then
/// the file streams into its row.
pub fn download(chat: &str, id: &str, filename: &str, cx: &mut App) {
    let name = if filename.is_empty() { "file".to_string() } else { filename.to_string() };
    let url = format!("/api/chat-media/file?session={chat}&id={id}");
    let chat = chat.to_string();
    if let Some(p) = crundi_core::driver::take_pick() { start_download(chat, url, name, p, cx); return; }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(std::path::PathBuf::from).unwrap_or_default();
    let dir = if home.join("Downloads").is_dir() { home.join("Downloads") } else { home };
    let rx = cx.prompt_for_new_path(&dir, Some(&name));
    cx.spawn(async move |cx: &mut AsyncApp| {
        if let Ok(Ok(Some(p))) = rx.await { let _ = cx.update(|cx| start_download(chat, url, name, p, cx)); }
    }).detach();
}

fn start_download(chat: String, url: String, name: String, path: std::path::PathBuf, cx: &mut App) {
    let Some(api) = crundi_shell::api(cx) else { return };
    let cancel = Arc::new(AtomicBool::new(false));
    let m = g(cx);
    m.seq += 1;
    let id = m.seq;
    m.dls.push(Dl { id, chat: chat.clone(), name, line: "Starting…".into(), state: "running".into(), frac: 0.0, cancel: cancel.clone() });
    redraw(&chat, cx);
    let prog = Arc::new(Mutex::new((0u64, 0u64, false)));
    let p2 = prog.clone();
    // The row's line, at most every 120 ms while it runs.
    let c2 = chat.clone();
    cx.spawn(async move |cx: &mut AsyncApp| loop {
        cx.background_executor().timer(Duration::from_millis(120)).await;
        let (got, total, fresh) = { let mut p = p2.lock().unwrap(); let r = *p; p.2 = false; r };
        let go = cx.update(|cx| {
            let Some(d) = g(cx).dls.iter_mut().find(|d| d.id == id) else { return false };
            if d.state != "running" { return false; }
            if fresh {
                d.line = if total > 0 { format!("{} of {}", media::fmt_dl_size(got), media::fmt_dl_size(total)) } else { media::fmt_dl_size(got) };
                d.frac = if total > 0 { got as f32 / total as f32 } else { 0.0 };
                redraw(&c2, cx);
            }
            true
        });
        if !go { break; }
    }).detach();
    crundi_shell::run(cx, move || stream_to(&api, &url, &path, &mut |g, t| { *prog.lock().unwrap() = (g, t, true); }, &cancel), move |r, cx| {
        let (state, line, linger) = match r {
            Ok(n) => ("done", format!("Saved · {}", media::fmt_dl_size(n)), LINGER_DONE),
            Err(e) if e == "\u{0}cancel" => ("cancelled", "Cancelled".to_string(), LINGER_CANCEL),
            Err(e) => ("failed", e, LINGER_FAIL),
        };
        if let Some(d) = g(cx).dls.iter_mut().find(|d| d.id == id) { d.state = state.into(); d.line = line; if state == "done" { d.frac = 1.0; } }
        redraw(&chat, cx);
        cx.spawn(async move |cx: &mut AsyncApp| {
            cx.background_executor().timer(Duration::from_millis(linger)).await;
            let _ = cx.update(|cx| { g(cx).dls.retain(|d| d.id != id); redraw(&chat, cx); });
        }).detach();
    });
}

/// A row's ×: a running download is cancelled; a finished row goes.
pub fn dl_close(id: u32, cx: &mut App) {
    let m = g(cx);
    let chat = m.dls.iter().find(|d| d.id == id).map(|d| d.chat.clone()).unwrap_or_default();
    if let Some(d) = m.dls.iter().find(|d| d.id == id && d.state == "running") { d.cancel.store(true, Ordering::Relaxed); return; }
    m.dls.retain(|d| d.id != id);
    redraw(&chat, cx);
}

/// The download rows, top right under the top bar: a row per download,
/// newest last; its left edge and icon accent (done green, failed red).
fn tray(chat: &str, window: &mut Window, cx: &mut Context<LogView>) -> Option<AnyElement> {
    let m = gr(cx)?;
    let rows: Vec<AnyElement> = m.dls.iter().filter(|d| d.chat == chat).map(|d| {
        let tone = match d.state.as_str() { "done" => T::green(), "failed" => T::red(), _ => T::accent() };
        let id = d.id;
        div().flex().items_center().gap(T::SP_10).pl(T::SP_12).pr(T::SP_10).py(T::SP_10).bg(T::bg_secondary()).border_1().border_color(T::border())
            .rounded(T::R_CONTROL).shadow_md().relative().overflow_hidden()
            // The left edge: 3, the row's tone.
            .child(div().absolute().left_0().top_0().bottom_0().w(T::SP_2 + px(1.)).bg(tone))
            .child(div().text_color(if d.state == "running" { T::accent_hover() } else { tone }).child(w::icon(IconName::Download, T::ICON_LG)))
            .child(div().flex_1().min_w_0().flex().flex_col().gap(T::SP_2)
                .child(div().text_size(T::TEXT_12).font_weight(T::W_SEMIBOLD).text_color(T::text()).truncate().child(d.name.clone()))
                .child(div().text_size(T::TEXT_11).text_color(T::text_secondary()).truncate().child(d.line.clone()))
                .when(d.state != "failed", |c| c.child(div().h(T::SP_2 + px(1.)).w_full().rounded(T::R_PILL).bg(T::bg_tertiary())
                    .child(div().h_full().w(relative(d.frac.clamp(0.0, 1.0))).rounded(T::R_PILL).bg(if d.state == "done" { T::green() } else { T::accent() })))))
            .child(w::icon_button(SharedString::from(format!("dlx-{id}")), IconName::X, if d.state == "running" { "Cancel" } else { "Close" }).on_click(move |_, _, cx| dl_close(id, cx)))
            .into_any_element()
    }).collect();
    if rows.is_empty() { return None; }
    Some(deferred(anchored().position(point(px(0.), T::TOPBAR_H + T::SP_48 + T::SP_16)).child(
        div().w(window.viewport_size().width).flex().justify_end().pr(T::SP_16).child(div().w(T::DIALOG_W - T::SP_40 - T::SP_20).flex().flex_col().gap(T::SP_8).children(rows))
    )).with_priority(1).into_any_element())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn lines_and_boxes() {
        let (it, _) = media::items(r#"{"items":[{"kind":"file","id":"0123456789abcdef","filename":"export.csv","size":12,"caption":"The export"},{"kind":"image","name":"0123456789abcdef.png","w":1200,"h":700}]}"#);
        assert_eq!(file_line(&it[0]), "12 B · The export");
        assert_eq!(box_of(&it[1], false), (640.0, 373.0));
        assert_eq!(box_of(&it[0], false), (0.0, 0.0));
        assert!(!several(&it));
    }

    #[::core::prelude::v1::test]
    fn zoom_and_pan() {
        assert_eq!(zoom_to(0.5), 1.0);
        assert_eq!(zoom_to(6.0 * ZOOM_STEP), ZOOM_MAX);
        // A 2000-wide picture in a 1000 room: up to 540 either way.
        assert_eq!(clamp_pan((900.0, 0.0), (2000.0, 500.0), (1000.0, 800.0)), (540.0, 0.0));
    }

    #[::core::prelude::v1::test]
    fn decodes_png() {
        let mut buf = vec![];
        let img = image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]));
        image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();
        let (_, w, h) = decode(&buf).unwrap();
        assert_eq!((w, h), (3, 2));
        assert!(decode(b"not a picture").is_none());
    }
}
