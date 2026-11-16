//! 2.7: attachments (specs/attachments-drafts.md; the Slint client's
//! src/app.rs "Attachments"): the attach button and the system's file
//! picker, files dropped from the system on a chat (their own paths on this
//! machine, else uploaded), a pasted picture (image.png) or copied file, the
//! upload bar (.cc-up: the name, the share sent, "N%" then "saving"), and the
//! server's path put into the message at the caret (POST
//! /api/attachments/upload). The rules are crundi_core::attach's.

use crate::ComposerView;
use crundi_core::attach as rules;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A chat's uploads: what its bar shows, and how many still run.
#[derive(Default)]
pub struct State {
    pub name: String,
    pub label: String,
    pub frac: f32,
    pub active: u32,
    /// Shown (also for a moment after the last ends), and which hide is due.
    pub shown: bool,
    hide_gen: u32,
}

/// The window-wide side: files dragged over the window, the pointer, each
/// composer's bounds (to find the chat under the pointer), the last toast.
#[derive(Default)]
pub struct Media {
    pub file_hover: bool,
    pub inside: bool,
    pub pointer: Point<Pixels>,
    pub bounds: HashMap<String, Bounds<Pixels>>,
    pub toast: String,
}
impl Global for Media {}

/// At start (lib.rs's init): the window-wide state.
pub fn init(cx: &mut App) {
    if !cx.has_global::<Media>() { cx.set_global(Media::default()); }
}

fn media(cx: &mut App) -> &mut Media {
    if !cx.has_global::<Media>() { cx.set_global(Media::default()); }
    cx.global_mut::<Media>()
}

/// Something in the first window (a toast, the box's focus) from a callback
/// that has only the App.
pub(crate) fn in_window(cx: &mut App, f: impl FnOnce(&mut Window, &mut App)) {
    if let Some(h) = cx.windows().first().copied() { let _ = cx.update_window(h, |_, window, cx| f(window, cx)); }
}

/// The toast (one record for the driver's props.toast: secrets.rs keeps it).
pub(crate) fn toast(cx: &mut App, text: &str, error: bool) {
    media(cx).toast = text.to_string();
    crate::secrets::toast(cx, text, if error { w::Toast::Error } else { w::Toast::Info });
}

/// Later, on the UI thread.
pub(crate) fn after(cx: &mut App, ms: u64, f: impl FnOnce(&mut App) + 'static) {
    cx.spawn(async move |cx: &mut AsyncApp| {
        cx.background_executor().timer(Duration::from_millis(ms)).await;
        let _ = cx.update(f);
    }).detach();
}

// ─── What it draws ───

/// An invisible layer over the composer: its bounds (the chat under a
/// dragged file) and the pointer in the window. Out of the flow.
pub fn render(c: &mut ComposerView, _window: &mut Window, cx: &mut Context<ComposerView>) -> Option<AnyElement> {
    let chat = c.chat_id(cx);
    Some(canvas(
        move |bounds, _, cx| { media(cx).bounds.insert(chat.clone(), bounds); },
        |_, _, window, _| {
            window.on_mouse_event(|e: &MouseMoveEvent, phase, _, cx| if phase == DispatchPhase::Capture { let m = media(cx); m.inside = true; m.pointer = e.position; });
            window.on_mouse_event(|_: &MouseExitEvent, phase, _, cx| if phase == DispatchPhase::Capture { media(cx).inside = false; });
        },
    ).absolute().size_full().into_any_element())
}

/// The attach button (.cc-attach): the paperclip; faint and inert while a
/// file uploads.
pub fn button(c: &mut ComposerView, _window: &mut Window, cx: &mut Context<ComposerView>) -> AnyElement {
    let busy = c.attach.active > 0;
    w::square_button("attach", IconName::Paperclip, "Attach a file (uploads to crundi_attachments)", if busy { w::SquareLook::Busy } else { w::SquareLook::Plain }, T::CONTROL, T::ICON_LG)
        .when(!busy, |d| d.on_click(cx.listener(|this, _, _, cx| pick(this, cx))))
        .into_any_element()
}

/// The upload bar (.cc-up): the file's name (elided) and the track sharing
/// the row 1:2, then the number or "saving".
pub fn strip(c: &mut ComposerView, _window: &mut Window, _cx: &mut Context<ComposerView>) -> Option<AnyElement> {
    let u = &c.attach;
    if !u.shown { return None; }
    let frac = u.frac.clamp(0.0, 1.0);
    Some(div().flex().items_center().gap(T::SP_8).px(T::SP_2).text_size(T::TEXT_11).text_color(T::text_secondary())
        .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().child(u.name.clone()))
        .child(div().flex_basis(relative(0.)).min_w_0().map(|mut d| { d.style().flex_grow = Some(2.); d }).h(T::SP_6).rounded(T::R_PILL).bg(T::bg_tertiary()).overflow_hidden()
            .child(div().h_full().w(relative(frac)).rounded(T::R_PILL).bg(T::accent())))
        .child(div().flex_none().font_family(T::MONO).text_color(T::text_muted()).child(u.label.clone()))
        .into_any_element())
}

// ─── Doing it ───

/// The attach button: the system's file picker (one file, any type), off
/// the UI thread; the picked file always uploads. The test driver answers
/// the picker ("pick-next").
pub fn pick(c: &mut ComposerView, cx: &mut Context<ComposerView>) {
    if c.attach.active > 0 { return; }
    let chat = c.chat_id(cx);
    crundi_shell::run(cx, || crundi_core::driver::take_pick().or_else(|| rfd::FileDialog::new().pick_file()), move |p, cx| {
        if let Some(p) = p { upload_file(cx, &chat, &p); }
    });
}

/// Upload a file from disk to a chat's project.
pub fn upload_file(cx: &mut App, chat: &str, path: &Path) {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    match std::fs::read(path) {
        Ok(bytes) => upload(cx, chat, name, bytes),
        Err(e) => toast(cx, &format!("Upload failed: {e}"), true),
    }
}

fn with_state(cx: &mut App, chat: &str, f: impl FnOnce(&mut State)) {
    if let Some(c) = crate::composer(cx, chat) { c.update(cx, |c, cx| { f(&mut c.attach); cx.notify(); }); }
}

/// Upload bytes as `name`: the bar, the upload off the UI thread (its
/// progress read back every 100 ms), then the path at the caret and a
/// toast, or the error's toast.
pub fn upload(cx: &mut App, chat: &str, name: String, bytes: Vec<u8>) {
    let project = crundi_shell::project(cx);
    if project.is_empty() { return toast(cx, "No project for this chat", true); }
    let Some(api) = crundi_shell::api(cx) else { return };
    with_state(cx, chat, |u| { u.active += 1; u.shown = true; u.name = name.clone(); u.label = "0%".into(); u.frac = 0.0; });
    let prog = Arc::new(Mutex::new((0u64, 0u64, false)));
    let p2 = prog.clone();
    let (chat_s, name_s) = (chat.to_string(), name.clone());
    // The bar follows the body going out.
    {
        let (prog, chat, name) = (prog.clone(), chat.to_string(), name.clone());
        cx.spawn(async move |cx: &mut AsyncApp| loop {
            cx.background_executor().timer(Duration::from_millis(100)).await;
            let (sent, total, done) = *prog.lock().unwrap();
            if done { break; }
            let (chat, name) = (chat.clone(), name.clone());
            let _ = cx.update(move |cx| with_state(cx, &chat, |u| {
                u.name = name; u.label = rules::progress_label(sent, total);
                u.frac = if total == 0 { 0.0 } else { (sent as f64 / total as f64) as f32 };
            }));
        }).detach();
    }
    crundi_shell::run(cx, move || {
        use base64::Engine;
        let body = serde_json::to_vec(&json!({ "project": project, "name": name, "data": base64::engine::general_purpose::STANDARD.encode(&bytes) })).unwrap_or_default();
        let progress = move |sent: u64, total: u64| { let mut g = p2.lock().unwrap(); g.0 = sent; g.1 = total; };
        let r = api.post_with_progress("/api/attachments/upload", &body, &progress);
        prog.lock().unwrap().2 = true;
        r
    }, move |r, cx| {
        let answer = match r {
            Err(crundi_core::api::ApiError::SignedOut) => return crundi_shell::session::sign_out(cx, "Your sign-in ended. Sign in again."),
            Err(_) => rules::upload_answer(0, "", &name_s),
            Ok((status, body)) => rules::upload_answer(status, &body, &name_s),
        };
        upload_done(cx, &chat_s, answer);
    });
}

fn upload_done(cx: &mut App, chat: &str, answer: Result<(String, String), String>) {
    let mut rgen = (1, 0);
    with_state(cx, chat, |u| { u.active = u.active.saturating_sub(1); u.hide_gen += 1; rgen = (u.active, u.hide_gen); });
    match answer {
        Ok((path, name)) => { insert_text(cx, chat, &path); toast(cx, &format!("Attached: {name}"), false); }
        Err(e) => toast(cx, &e, true),
    }
    // The bar stays a moment after the chat's last upload, then goes.
    if rgen.0 == 0 {
        let (chat, g) = (chat.to_string(), rgen.1);
        after(cx, 350, move |cx| with_state(cx, &chat, |u| if u.active == 0 && u.hide_gen == g { *u = State::default(); u.hide_gen = g; }));
    }
}

/// Put text (a path) into a chat's box at its caret, replacing a selection,
/// spaced as the web's; the box takes the focus with the caret after it.
pub fn insert_text(cx: &mut App, chat: &str, path: &str) {
    let Some(c) = crate::composer(cx, chat) else { return };
    let input = c.read(cx).input.clone();
    let path = path.to_string();
    in_window(cx, move |window, cx| input.update(cx, |st, cx| {
        let text = st.value().to_string();
        let r = st.selected_range();
        let (new, caret) = rules::insert_path(&text, r.start, r.end, &path);
        st.set_value(new, window, cx);
        st.set_selected_range(caret..caret, cx);
        st.focus(window, cx);
    }));
}

/// Text into the box at the caret, replacing a selection, as typed (Shift+Insert).
pub fn put_text(c: &mut ComposerView, text: &str, window: &mut Window, cx: &mut Context<ComposerView>) {
    c.input.update(cx, |st, cx| {
        let cur = st.value().to_string();
        let r = st.selected_range();
        let fix = |i: usize| { let mut i = i.min(cur.len()); while !cur.is_char_boundary(i) { i -= 1; } i };
        let (a, b) = (fix(r.start.min(r.end)), fix(r.start.max(r.end)));
        let new = format!("{}{}{}", &cur[..a], text, &cur[b..]);
        st.set_value(new, window, cx);
        st.set_selected_range(a + text.len()..a + text.len(), cx);
    });
}

/// Files dropped on a chat: on this machine, their own paths go in (one
/// insertion, a toast); otherwise each uploads. `upload_anyway`: as on a
/// remote server (the test driver's).
pub fn drop_files(cx: &mut App, chat: &str, paths: Vec<PathBuf>, upload_anyway: bool) {
    if paths.is_empty() || crate::composer(cx, chat).is_none() { return; }
    let local = crundi_shell::api(cx).is_some_and(|a| rules::is_local(a.base()));
    if local && !upload_anyway {
        let joined = paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join(" ");
        insert_text(cx, chat, &joined);
        toast(cx, &rules::added_toast(paths.len()), false);
    } else {
        for p in paths { upload_file(cx, chat, &p); }
    }
}

/// The chat under the pointer while files are dragged over the window
/// (each composer's column: its bounds, up to its pane's height).
fn chat_under_pointer(cx: &mut App) -> Option<String> {
    let m = media(cx);
    if !(m.file_hover && m.inside) { return None; }
    let (p, bounds) = (m.pointer, m.bounds.clone());
    let panes = crundi_workbench::workbench(cx).map(|w| w.update(cx, |w, cx| w.panes(cx))).unwrap_or_default();
    bounds.into_iter().find(|(chat, b)| {
        let h = panes.iter().find(|q| q.get("key").and_then(|k| k.as_str()) == Some(&format!("live:{chat}"))).and_then(|q| q.get("h")).and_then(|h| h.as_f64()).unwrap_or(0.0) as f32;
        let (x0, x1, y1) = (f32::from(b.origin.x), f32::from(b.origin.x + b.size.width), f32::from(b.origin.y + b.size.height));
        let (px_, py) = (f32::from(p.x), f32::from(p.y));
        px_ >= x0 && px_ <= x1 && py <= y1 && py >= y1 - h.max(f32::from(b.size.height))
    }).map(|(c, _)| c)
}

/// Files dropped as the system delivers them: on the chat the drag is over,
/// else the focused chat. (A real drop reaches `drop_target` too.)
pub fn os_drop(cx: &mut App, paths: Vec<PathBuf>) {
    let chat = chat_under_pointer(cx).unwrap_or_else(|| crate::focused_chat(cx));
    media(cx).file_hover = false;
    drop_files(cx, &chat, paths, false);
}

/// For the chat pane (the chats crate): the system's file drops on the
/// whole chat, with the dashed accent outline while a file is over it.
pub fn drop_target(el: Stateful<Div>, chat: &str) -> Stateful<Div> {
    let chat = chat.to_string();
    el.drag_over::<ExternalPaths>(|s, _, _, _| s.border_2().border_dashed().border_color(T::accent()))
        .on_drop(move |p: &ExternalPaths, _, cx| { let paths = p.paths().to_vec(); drop_files(cx, &chat, paths, false); })
}

/// The box's paste (Textarea::on_paste): a picture uploads as image.png, a
/// copied file uploads; anything else is the ordinary paste.
pub fn paste_handler(cx: &mut Context<ComposerView>) -> impl Fn(&ClipboardItem, &mut Window, &mut App) -> bool + 'static {
    let me = cx.entity().downgrade();
    move |item, _, cx| {
        let Some(chat) = me.upgrade().map(|c| c.read(cx).chat_id(cx)) else { return false };
        for e in item.entries() {
            match e {
                ClipboardEntry::ExternalPaths(p) => { if let Some(f) = p.paths().first().cloned() { upload_file(cx, &chat, &f); return true; } }
                ClipboardEntry::Image(img) => {
                    let name = match img.format { ImageFormat::Png => "image.png".to_string(), f => format!("image.{}", format!("{f:?}").to_lowercase()) };
                    upload(cx, &chat, name, img.bytes.clone());
                    return true;
                }
                _ => {}
            }
        }
        false
    }
}

/// Shift+Insert in the box: the clipboard's text at the caret (F57; the
/// box pastes on Ctrl+V only). True when it took the key.
pub fn shift_insert(c: &mut ComposerView, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<ComposerView>) -> bool {
    let k = &ev.keystroke;
    if k.key != "insert" || !k.modifiers.shift || k.modifiers.control { return false; }
    if let Some(t) = cx.read_from_clipboard().and_then(|i| i.text()) { put_text(c, &t, window, cx); }
    true
}

// ─── The test driver ───

pub fn test_ui(c: &mut ComposerView, name: &str, args: &[String], window: &mut Window, cx: &mut Context<ComposerView>) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let chat = c.chat_id(cx);
    let all = || args.iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>();
    match name {
        "attach-button" => pick(c, cx),
        // A file as if picked in the dialog, or pasted.
        "attach" | "paste-file" => { let p = a(0); cx.defer(move |cx| upload_file(cx, &chat, Path::new(&p))); }
        "paste-image" => match std::fs::read(a(0)) {
            Ok(b) => cx.defer(move |cx| upload(cx, &chat, "image.png".into(), b)),
            Err(e) => return Some(json!({ "ok": false, "error": e.to_string() })),
        },
        // Dropped on the focused chat ("upload" first: as on a remote server).
        "drop" => {
            let v = all();
            let force = v.first().is_some_and(|x| x == "upload");
            let paths: Vec<PathBuf> = v.into_iter().filter(|x| x != "upload").map(PathBuf::from).collect();
            cx.defer(move |cx| drop_files(cx, &chat, paths, force));
        }
        // Dropped as the system delivers them, on the chat under the pointer.
        "os-drop" => { let paths: Vec<PathBuf> = all().into_iter().map(PathBuf::from).collect(); cx.defer(move |cx| os_drop(cx, paths)); }
        "file-hover" => media(cx).file_hover = a(0) == "true",
        "clipboard-text" => cx.write_to_clipboard(ClipboardItem::new_string(a(0))),
        // The caret (and selection) in the box, byte offsets.
        "caret" => {
            let at: usize = a(0).parse().unwrap_or(0);
            let to: usize = a(1).parse().unwrap_or(at);
            c.input.update(cx, |st, cx| { let n = st.value().len(); st.focus(window, cx); st.set_selected_range(at.min(to).min(n)..at.max(to).min(n), cx); });
        }
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

pub fn pane_props(c: &ComposerView, _cx: &App) -> Value {
    let u = &c.attach;
    json!({ "upOn": u.shown, "upName": u.name, "upLabel": u.label, "upBusy": u.active > 0 })
}

pub fn props(_c: &ComposerView, _cx: &App) -> Value { json!({}) }
