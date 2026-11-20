//! 2.7: attach (the web's .cc-attach, the paste and drop handlers, .cc-up):
//! the paperclip and the system's file picker, files dropped from the
//! system on the chat (their own paths when the server is on this machine,
//! else uploaded), a pasted picture (image.png) or copied file, the upload
//! bar (the name, the share sent, "N%" then "saving"), and the server's path
//! put into the message at the caret (POST /api/attachments/upload). The
//! rules are crundi_core::attach's.

use super::Composer;
use crundi_core::attach as rules;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
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

/// The attach button: faint and inert while a file uploads.
pub fn button(c: &mut Composer, cx: &mut Context<Composer>) -> AnyElement {
    let busy = c.attach.active > 0;
    w::square_button("attach", IconName::Paperclip, "Attach a file (uploads to crundi_attachments)", if busy { w::SquareLook::Busy } else { w::SquareLook::Plain }, T::CONTROL, T::ICON_LG)
        .when(!busy, |d| d.on_click(cx.listener(|this, _, _, cx| pick(this, cx))))
        .into_any_element()
}

/// The upload bar: the file's name (elided) and the track sharing the row
/// 1:2, then the number or "saving".
pub fn strip(c: &Composer) -> Option<AnyElement> {
    let u = &c.attach;
    if !u.shown { return None; }
    let frac = u.frac.clamp(0.0, 1.0);
    let fs = c.fs();
    Some(w::layer(div().flex().items_center().gap(T::SP_8).pt(T::SP_4).px(T::SP_2).text_size(T::TEXT_11 * fs).text_color(T::text_muted())
        .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().child(u.name.clone()))
        .child(div().flex_basis(relative(0.)).min_w_0().map(|mut d| { d.style().flex_grow = Some(2.); d }).h(T::SP_4).rounded(T::R_PILL).bg(T::bg_tertiary()).overflow_hidden()
            .child(div().h_full().w(relative(frac)).rounded(T::R_PILL).bg(T::accent())))
        .child(div().flex_none().min_w(T::SP_32).flex().justify_end().child(u.label.clone()))).into_any_element())
}

/// The attach button: the system's file picker (one file, any type), off
/// the UI thread; the picked file always uploads. The driver answers the
/// picker (its "pick-next").
pub fn pick(c: &mut Composer, cx: &mut Context<Composer>) {
    if c.attach.active > 0 { return; }
    let me = cx.entity();
    crundi_shell::run(cx, || crundi_core::driver::take_pick().or_else(|| rfd::FileDialog::new().pick_file()), move |p, cx| {
        if let Some(p) = p { upload_file(&me, &p, cx); }
    });
}

/// Upload a file from disk to the chat's project.
pub fn upload_file(c: &Entity<Composer>, path: &Path, cx: &mut App) {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    match std::fs::read(path) {
        Ok(bytes) => upload(c, name, bytes, cx),
        Err(e) => super::toast(cx, &format!("Upload failed: {e}"), true),
    }
}

fn with_state(c: &WeakEntity<Composer>, cx: &mut App, f: impl FnOnce(&mut State)) {
    let _ = c.update(cx, |c, cx| { f(&mut c.attach); cx.notify(); });
}

/// Upload bytes as `name`: the bar, the upload off the UI thread (its
/// progress read back every 100 ms while it runs), then the path at the
/// caret and a toast, or the error's toast.
pub fn upload(c: &Entity<Composer>, name: String, bytes: Vec<u8>, cx: &mut App) {
    let project = crundi_shell::project(cx);
    if project.is_empty() { return super::toast(cx, "No project for this chat", true); }
    let Some(api) = crundi_shell::api(cx) else { return };
    let weak = c.downgrade();
    with_state(&weak, cx, |u| { u.active += 1; u.shown = true; u.name = name.clone(); u.label = "0%".into(); u.frac = 0.0; });
    let prog = Arc::new(Mutex::new((0u64, 0u64, false)));
    let p2 = prog.clone();
    {
        let (prog, weak, name) = (prog.clone(), weak.clone(), name.clone());
        cx.spawn(async move |cx: &mut AsyncApp| loop {
            cx.background_executor().timer(Duration::from_millis(100)).await;
            let (sent, total, done) = *prog.lock().unwrap();
            if done { break; }
            let (weak, name) = (weak.clone(), name.clone());
            let _ = cx.update(move |cx| with_state(&weak, cx, |u| {
                let label = rules::progress_label(sent, total);
                let frac = if total == 0 { 0.0 } else { (sent as f64 / total as f64) as f32 };
                if u.label != label || u.frac != frac { u.name = name; u.label = label; u.frac = frac; }
            }));
        }).detach();
    }
    let name_s = name.clone();
    crundi_shell::run(cx, move || {
        use base64::Engine;
        let body = serde_json::to_vec(&json!({ "project": project, "name": name, "data": base64::engine::general_purpose::STANDARD.encode(&bytes) })).unwrap_or_default();
        // Under the test driver only, CRUNDI_TEST_UPLOAD_KBPS slows the
        // upload to that rate, so a test can watch the bar with a small file.
        let rate = test_upload_rate();
        let started = std::time::Instant::now();
        let progress = move |sent: u64, total: u64| {
            { let mut g = p2.lock().unwrap(); g.0 = sent; g.1 = total; }
            if let Some(bps) = rate {
                let due = std::time::Duration::from_secs_f64(sent as f64 / bps);
                if let Some(wait) = due.checked_sub(started.elapsed()) { std::thread::sleep(wait); }
            }
        };
        let r = api.post_with_progress("/api/attachments/upload", &body, &progress);
        prog.lock().unwrap().2 = true;
        r
    }, move |r, cx| {
        let answer = match r {
            Err(crundi_core::api::ApiError::SignedOut) => {
                let s = crundi_shell::stores(cx);
                return s.session.update(cx, |s, cx| s.sign_out("Your sign-in ended. Sign in again.", cx));
            }
            Err(_) => rules::upload_answer(0, "", &name_s),
            Ok((status, body)) => rules::upload_answer(status, &body, &name_s),
        };
        upload_done(&weak, answer, cx);
    });
}

fn upload_done(c: &WeakEntity<Composer>, answer: Result<(String, String), String>, cx: &mut App) {
    let mut rgen = (1, 0);
    with_state(c, cx, |u| { u.active = u.active.saturating_sub(1); u.hide_gen += 1; rgen = (u.active, u.hide_gen); });
    match answer {
        Ok((path, name)) => { if let Some(e) = c.upgrade() { insert_text(&e, &path, cx); } super::toast(cx, &format!("Attached: {name}"), false); }
        Err(e) => super::toast(cx, &e, true),
    }
    // The bar stays a moment after the chat's last upload, then goes.
    if rgen.0 == 0 {
        let (c, g) = (c.clone(), rgen.1);
        super::after(cx, 350, move |cx| with_state(&c, cx, |u| if u.active == 0 && u.hide_gen == g { *u = State::default(); u.hide_gen = g; }));
    }
}

/// Text (a path) into the box at its caret, replacing a selection, spaced
/// as the web's; the box takes the focus with the caret after it.
pub fn insert_text(c: &Entity<Composer>, path: &str, cx: &mut App) {
    let input = c.read(cx).input.clone();
    let path = path.to_string();
    super::in_window(cx, move |window, cx| input.update(cx, |st, cx| {
        let text = st.value().to_string();
        let r = st.selected_range();
        let (new, caret) = rules::insert_path(&text, r.start, r.end, &path);
        st.set_value(new, window, cx);
        st.set_selected_range(caret..caret, cx);
        st.focus(window, cx);
    }));
    // (set_value does not say it changed: the draft follows here.)
    let c = c.clone();
    cx.defer(move |cx| c.update(cx, |c, cx| c.text_edited(cx)));
}

/// Text into the box at the caret, replacing a selection, as typed (Shift+Insert).
fn put_text(c: &mut Composer, text: &str, window: &mut Window, cx: &mut Context<Composer>) {
    c.input.update(cx, |st, cx| {
        let cur = st.value().to_string();
        let r = st.selected_range();
        let fix = |i: usize| { let mut i = i.min(cur.len()); while !cur.is_char_boundary(i) { i -= 1; } i };
        let (a, b) = (fix(r.start.min(r.end)), fix(r.start.max(r.end)));
        st.set_value(format!("{}{}{}", &cur[..a], text, &cur[b..]), window, cx);
        st.set_selected_range(a + text.len()..a + text.len(), cx);
    });
    c.text_edited(cx);
}

/// Files dropped on a chat: on this machine, their own paths go in (one
/// insertion, a toast); otherwise each uploads. `upload_anyway`: as on a
/// remote server (the driver's).
pub fn drop_files(c: &Entity<Composer>, paths: Vec<PathBuf>, upload_anyway: bool, cx: &mut App) {
    if paths.is_empty() { return; }
    let local = crundi_shell::api(cx).is_some_and(|a| rules::is_local(a.base()));
    if local && !upload_anyway {
        let joined = paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join(" ");
        insert_text(c, &joined, cx);
        super::toast(cx, &rules::added_toast(paths.len()), false);
    } else {
        for p in paths { upload_file(c, &p, cx); }
    }
}

/// The box's paste: a picture uploads as image.png, a copied file uploads;
/// anything else is the ordinary paste.
pub fn paste_handler(cx: &mut Context<Composer>) -> impl Fn(&ClipboardItem, &mut Window, &mut App) -> bool + 'static {
    let me = cx.entity().downgrade();
    move |item, _, cx| {
        let Some(c) = me.upgrade() else { return false };
        for e in item.entries() {
            match e {
                ClipboardEntry::ExternalPaths(p) => { if let Some(f) = p.paths().first().cloned() { upload_file(&c, &f, cx); return true; } }
                ClipboardEntry::Image(img) => {
                    let name = match img.format { ImageFormat::Png => "image.png".to_string(), f => format!("image.{}", format!("{f:?}").to_lowercase()) };
                    upload(&c, name, img.bytes.clone(), cx);
                    return true;
                }
                _ => {}
            }
        }
        false
    }
}

/// Shift+Insert in the box: the clipboard's text at the caret (the box
/// pastes on Ctrl+V only). True when it took the key.
pub fn shift_insert(c: &mut Composer, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Composer>) -> bool {
    let k = &ev.keystroke;
    if k.key != "insert" || !k.modifiers.shift || k.modifiers.control { return false; }
    if let Some(t) = cx.read_from_clipboard().and_then(|i| i.text()) { put_text(c, &t, window, cx); }
    true
}

// ─── The test driver ───

pub fn test_ui(c: &mut Composer, name: &str, args: &[String], window: &mut Window, cx: &mut Context<Composer>) -> Option<Value> {
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    let me = cx.entity();
    let all = || args.iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>();
    match name {
        "attach-button" => pick(c, cx),
        // A file as if pasted (a copied file).
        "paste-file" => { let p = a(0); cx.defer(move |cx| upload_file(&me, Path::new(&p), cx)); }
        "paste-image" => match std::fs::read(a(0)) {
            Ok(b) => cx.defer(move |cx| upload(&me, "image.png".into(), b, cx)),
            Err(e) => return Some(json!({ "ok": false, "error": e.to_string() })),
        },
        // Dropped on this chat ("upload" first: as on a remote server).
        "drop-files" => {
            let v = all();
            let force = v.first().is_some_and(|x| x == "upload");
            let paths: Vec<PathBuf> = v.into_iter().filter(|x| x != "upload").map(PathBuf::from).collect();
            cx.defer(move |cx| drop_files(&me, paths, force, cx));
        }
        // The caret (and selection) in the box, byte offsets.
        "composer-caret" => {
            let at: usize = a(0).parse().unwrap_or(0);
            let to: usize = a(1).parse().unwrap_or(at);
            c.input.update(cx, |st, cx| { let n = st.value().len(); st.focus(window, cx); st.set_selected_range(at.min(to).min(n)..at.max(to).min(n), cx); });
        }
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

pub fn state(c: &Composer, _cx: &App) -> Value {
    let u = &c.attach;
    json!({ "upOn": u.shown, "upName": u.name, "upLabel": u.label, "upFrac": u.frac, "upBusy": u.active > 0 })
}

/// The test's upload rate in bytes a second (CRUNDI_TEST_UPLOAD_KBPS), only
/// with the test driver on (CRUNDI_TEST_PORT); None otherwise.
fn test_upload_rate() -> Option<f64> {
    std::env::var("CRUNDI_TEST_PORT").ok()?;
    let kbps: f64 = std::env::var("CRUNDI_TEST_UPLOAD_KBPS").ok()?.parse().ok()?;
    (kbps > 0.0).then_some(kbps * 1024.0)
}
