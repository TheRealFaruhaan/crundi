//! The file windows (the web's fe* window manager): floating, non-modal
//! windows over the app, each an editor (a file, highlighted; Save and
//! Ctrl+S), a viewer (a picture) or a Git diff (two columns; Stage a chunk;
//! ↑ ↓ between chunks). Dragged by the header with edge snapping (left,
//! right, top: full, the corners: quarters), resized from the right, the
//! bottom and the corner, expanded; the Arrange bar (Tile, Grid, Cascade)
//! while one moves and a moment after.

use super::{diff, g, hl, pal};
use crate::{kit, MainWindow, WorkspaceDiffRow, WorkspaceLine, WorkspaceSnap, WorkspaceWin};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, SharedString, StyledText, VecModel};
use std::{cell::RefCell, collections::HashSet, rc::Rc};

/// The top the windows keep under (the top bar), the smallest size.
const TOP: f32 = 48.0;
const MIN_W: f32 = 360.0;
const MIN_H: f32 = 200.0;
/// Past this many lines a file is edited without colours (the editor
/// draws each line; CodeMirror only those in view).
const HL_MAX_LINES: usize = 4000;

#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Rect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

struct Win {
    id: i32,
    kind: String,
    title: String,
    r: Rect,
    max: bool,
    project: String,
    file: String,
    original: String,
    text: String,
    text_gen: i32,
    read_only: bool,
    save_label: String,
    lang: hl::Lang,
    lines: Rc<VecModel<WorkspaceLine>>,
    line_src: Vec<String>,
    loading: String,
    img: Option<slint::Image>,
    ops: Vec<diff::Op>,
    chunks: Vec<diff::Chunk>,
    chunk_rows: Vec<usize>,
    staged: HashSet<usize>,
    cur: i32,
    rows: Rc<VecModel<WorkspaceDiffRow>>,
    jump_row: i32,
    jump_gen: i32,
}

#[derive(Default)]
struct St {
    wins: Vec<Win>,
    next: i32,
    cascade: u32,
    model: Rc<VecModel<WorkspaceWin>>,
    /// A drag or a resize: the window, where the pointer started, the rect then.
    drag: Option<(i32, f32, f32, Rect)>,
    zone: Option<&'static str>,
    arrange: bool,
    arrange_hover: bool,
    arrange_gen: u64,
}

thread_local! { static ST: RefCell<St> = RefCell::new(St { next: 1, ..Default::default() }); }

fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { ST.with(|s| f(&mut s.borrow_mut())) }

/// The window's logical size.
fn view() -> (f32, f32) {
    kit::ui().map(|w| { let win = w.window(); let s = win.scale_factor(); let p = win.size(); (p.width as f32 / s, p.height as f32 / s) }).unwrap_or((1400.0, 860.0))
}

/// A new window's place (feBuildWindow): min(900, 80%) by min(72%, 820),
/// each next one 28 down and right, six then round again.
pub fn new_rect(n: u32, vw: f32, vh: f32) -> Rect {
    let step = 28.0 * (n % 6) as f32;
    let w = (vw * 0.8).min(900.0).max(MIN_W);
    let h = (vh * 0.72).min(820.0).max(MIN_H);
    Rect { x: (vw * 0.18).max(40.0) + step, y: 54.0 + step, w, h }
}

/// The snap zone at a pointer (feZoneAt): 36 from an edge.
pub fn zone_at(x: f32, y: f32, vw: f32, vh: f32) -> Option<&'static str> {
    let e = 36.0;
    let (l, r, t, b) = (x < e, x > vw - e, y < e, y > vh - e);
    if t && l { return Some("tl"); } if t && r { return Some("tr"); }
    if b && l { return Some("bl"); } if b && r { return Some("br"); }
    if l { return Some("left"); } if r { return Some("right"); } if t { return Some("max"); }
    None
}

/// A zone's rect (feSnapRect).
pub fn snap_rect(zone: &str, vw: f32, vh: f32) -> Option<Rect> {
    let h = vh - TOP - 4.0;
    let w2 = vw / 2.0;
    let hh = h / 2.0 - 2.0;
    Some(match zone {
        "left" => Rect { x: 4.0, y: TOP, w: w2 - 6.0, h },
        "right" => Rect { x: w2 + 2.0, y: TOP, w: w2 - 6.0, h },
        "max" => Rect { x: 4.0, y: TOP, w: vw - 8.0, h },
        "tl" => Rect { x: 4.0, y: TOP, w: w2 - 6.0, h: hh },
        "tr" => Rect { x: w2 + 2.0, y: TOP, w: w2 - 6.0, h: hh },
        "bl" => Rect { x: 4.0, y: TOP + h / 2.0 + 2.0, w: w2 - 6.0, h: hh },
        "br" => Rect { x: w2 + 2.0, y: TOP + h / 2.0 + 2.0, w: w2 - 6.0, h: hh },
        _ => return None,
    })
}

/// Arrange's rects (feArrange) for n windows.
pub fn arrange_rects(mode: &str, n: usize, vw: f32, vh: f32) -> Vec<Rect> {
    let (gap, h) = (6.0, vh - TOP - 4.0);
    match mode {
        "cascade" => {
            let (dw, dh) = ((vw * 0.7).min(900.0), (vh * 0.7).min(760.0));
            (0..n).map(|i| { let off = 28.0 * (i % 8) as f32; Rect { x: 40.0 + off, y: TOP + off, w: dw, h: dh } }).collect()
        }
        "tile" => {
            let cw = (vw - gap) / n.max(1) as f32;
            (0..n).map(|i| Rect { x: gap / 2.0 + i as f32 * cw, y: TOP, w: cw - gap, h }).collect()
        }
        _ => {
            let cols = (n as f32).sqrt().ceil().max(1.0) as usize;
            let rows = n.div_ceil(cols).max(1);
            let cw = (vw - gap) / cols as f32;
            let ch = (h - gap * (rows as f32 - 1.0)) / rows as f32;
            (0..n).map(|i| { let (c, r) = (i % cols, i / cols); Rect { x: gap / 2.0 + c as f32 * cw, y: TOP + r as f32 * (ch + gap), w: cw - gap, h: ch } }).collect()
        }
    }
}

const IMAGE_EXTS: [&str; 9] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg", "avif"];

fn ext_of(f: &str) -> String { f.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default() }

pub fn init(w: &MainWindow) {
    let ws = w.global::<crate::WorkspaceUi>();
    with(|s| ws.set_wins(ModelRc::from(s.model.clone())));
    ws.on_win_front(front);
    ws.on_win_close(close);
    ws.on_win_expand(expand);
    ws.on_win_save(|id, t| save(id, &t));
    ws.on_win_edited(|id, t| edited(id, &t));
    ws.on_win_drag(|id, ph, x, y| drag(id, ph, x, y));
    ws.on_win_resize(|id, ph, x, y| resize(id, ph, x, y));
    ws.on_win_arrange(|m| arrange(&m));
    ws.on_arrange_hover(|on| { with(|s| s.arrange_hover = on); if on { show_arrange(None); } else { show_arrange(Some(1200)); } });
    ws.on_diff_stage(diff_stage);
    ws.on_diff_step(diff_step);
}

fn view_of(w: &Win) -> WorkspaceWin {
    let (vw, vh) = view();
    let r = if w.max { Rect { x: 6.0, y: TOP, w: vw - 12.0, h: vh - TOP - 6.0 } } else { w.r };
    let n = w.chunks.len();
    WorkspaceWin {
        id: w.id, kind: w.kind.clone().into(), title: w.title.clone().into(),
        x: r.x, y: r.y, w: r.w, h: r.h, max: w.max,
        save_shown: w.kind == "edit" && !w.read_only,
        save_on: w.text != w.original && w.save_label == "Save",
        save_label: w.save_label.clone().into(),
        text: w.text.clone().into(), text_gen: w.text_gen,
        lines: ModelRc::from(w.lines.clone()), read_only: w.read_only,
        loading: w.loading.clone().into(),
        has_img: w.img.is_some(), img: w.img.clone().unwrap_or_default(),
        rows: ModelRc::from(w.rows.clone()),
        counter: if w.cur < 0 { format!("\u{2014}/{n}") } else { format!("{}/{n}", w.cur + 1) }.into(),
        jump_row: w.jump_row, jump_gen: w.jump_gen,
        file: w.file.clone().into(),
    }
}

/// The windows on the screen as they stand (front last).
fn show() {
    let rows: Vec<WorkspaceWin> = with(|s| s.wins.iter().map(view_of).collect());
    with(|s| {
        // Rows rewritten where they changed (their inner lists are shared).
        let m = &s.model;
        for (i, r) in rows.iter().enumerate() {
            if i < m.row_count() { if m.row_data(i).as_ref() != Some(r) { m.set_row_data(i, r.clone()); } } else { m.push(r.clone()); }
        }
        while m.row_count() > rows.len() { m.remove(m.row_count() - 1); }
    });
}

fn find<R>(id: i32, f: impl FnOnce(&mut Win) -> R) -> Option<R> { with(|s| s.wins.iter_mut().find(|w| w.id == id).map(f)) }

fn new_win(kind: &str, title: &str) -> i32 {
    let (vw, vh) = view();
    let id = with(|s| {
        let id = s.next; s.next += 1;
        let r = new_rect(s.cascade, vw, vh);
        s.cascade += 1;
        s.wins.push(Win {
            id, kind: kind.into(), title: title.into(), r, max: false, project: String::new(), file: String::new(),
            original: String::new(), text: String::new(), text_gen: 0, read_only: kind != "edit", save_label: "Save".into(),
            lang: hl::Lang::None, lines: Rc::new(VecModel::default()), line_src: vec![], loading: String::new(), img: None,
            ops: vec![], chunks: vec![], chunk_rows: vec![], staged: HashSet::new(), cur: -1,
            rows: Rc::new(VecModel::default()), jump_row: 0, jump_gen: 0,
        });
        id
    });
    show();
    id
}

pub fn front(id: i32) {
    let moved = with(|s| {
        let i = s.wins.iter().position(|w| w.id == id)?;
        if i + 1 == s.wins.len() { return None; }
        let w = s.wins.remove(i);
        s.wins.push(w);
        Some(())
    });
    if moved.is_some() { show(); }
}

pub fn close(id: i32) { with(|s| s.wins.retain(|w| w.id != id)); show(); }
pub fn close_all() { with(|s| s.wins.clear()); show(); }

pub fn expand(id: i32) { find(id, |w| w.max = !w.max); front(id); show(); }

/// Each line highlighted, the model rewritten where a line changed.
fn rehighlight(w: &mut Win) {
    let lines = w.text.split('\n').count();
    if lines > HL_MAX_LINES || w.kind != "edit" {
        if !w.line_src.is_empty() { w.line_src.clear(); w.lines.set_vec(vec![]); }
        return;
    }
    let p = pal();
    let colour = |t: hl::Tok| p.hl.iter().find(|(k, _)| *k == t).map(|(_, c)| c.clone()).unwrap_or_else(|| p.text.clone());
    let src = hl::highlight(w.lang, &w.text, &colour);
    for (i, m) in src.iter().enumerate() {
        if w.line_src.get(i) == Some(m) { continue; }
        let st = StyledText::from_markdown(m).unwrap_or_default();
        if i < w.lines.row_count() { w.lines.set_row_data(i, WorkspaceLine { md: st }); } else { w.lines.push(WorkspaceLine { md: st }); }
    }
    while w.lines.row_count() > src.len() { w.lines.remove(w.lines.row_count() - 1); }
    w.line_src = src;
}

/// Open a file (feOpen): a picture in a viewer (PDF in the system's viewer:
/// the editor windows have no PDF view); anything else in the editor (one
/// window a file: an open one comes to the front). `as_source`: an SVG's
/// text, not its picture.
pub fn open_file(project: &str, file: &str, as_source: bool) {
    let ext = ext_of(file);
    if !as_source && ext == "pdf" {
        let name = super::files::base_name(file);
        let url = format!("/api/files/download?project={}&file={}", kit::enc(project), kit::enc(file));
        return crate::app::open_fetched(url, name);
    }
    if !as_source && IMAGE_EXTS.contains(&ext.as_str()) { return open_viewer(project, file, &ext); }
    if let Some(id) = with(|s| s.wins.iter().find(|w| w.kind == "edit" && w.project == project && w.file == file).map(|w| w.id)) { return front(id); }
    let path = format!("/api/files/read?project={}&file={}", kit::enc(project), kit::enc(file));
    let (project, file) = (project.to_string(), file.to_string());
    kit::call(move |api| api.get(&path), move |r| match r {
        Ok(d) if d.get("ok").is_some_and(kit::truthy) => {
            let content = kit::s(&d, "content");
            let id = new_win("edit", &file);
            find(id, |w| {
                w.project = project.clone(); w.file = file.clone(); w.original = content.clone(); w.text = content.clone();
                w.read_only = false; w.text_gen += 1; w.lang = hl::lang_of(&file);
                rehighlight(w);
            });
            show();
        }
        Ok(d) => kit::toast_error(&{ let e = kit::s(&d, "error"); if e.is_empty() { "Cannot open file".into() } else { e } }),
        Err(e) => kit::toast_error(&format!("Error: {e}")),
    });
}

/// A picture (feOpenViewer): a short-lived link, then its bytes.
fn open_viewer(project: &str, file: &str, ext: &str) {
    let id = new_win("view", file);
    find(id, |w| w.loading = "Loading\u{2026}".into());
    show();
    let body = json!({ "project": project, "file": file });
    let svg = ext == "svg";
    kit::call(move |api| {
        let d = api.post("/api/files/download-link", &body)?;
        if !d.get("ok").is_some_and(kit::truthy) { return Ok(d); }
        let url = format!("{}?inline=1", kit::s(&d, "url"));
        let (st, bytes) = api.get_bytes(&url)?;
        if st != 200 { return Ok(json!({ "ok": false, "error": "Cannot open file" })); }
        use base64::Engine;
        Ok(json!({ "ok": true, "b64": base64::engine::general_purpose::STANDARD.encode(bytes) }))
    }, move |r| {
        let decoded = match &r {
            Ok(v) if v.get("ok").is_some_and(kit::truthy) => {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD.decode(kit::s(v, "b64")).unwrap_or_default();
                if svg { slint::Image::load_from_svg_data(&bytes).ok() } else {
                    image::load_from_memory(&bytes).ok().map(|img| {
                        let rgba = img.to_rgba8();
                        let mut pb = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(rgba.width(), rgba.height());
                        pb.make_mut_bytes().copy_from_slice(rgba.as_raw());
                        slint::Image::from_rgba8(pb)
                    })
                }.ok_or_else(|| "Cannot open file".to_string())
            }
            Ok(v) => Err({ let e = kit::s(v, "error"); if e.is_empty() { "Cannot open file".into() } else { e } }),
            Err(e) => Err(format!("Error: {e}")),
        };
        find(id, |w| match decoded { Ok(img) => { w.img = Some(img); w.loading.clear(); } Err(e) => w.loading = e });
        show();
    });
}

/// The editor's text changed: colours again, Save on when it differs.
pub fn edited(id: i32, text: &str) {
    let changed = find(id, |w| {
        let was = w.text != w.original;
        w.text = text.to_string();
        rehighlight(w);
        was != (w.text != w.original)
    });
    if changed.is_some() { show(); }
}

/// Save (and Ctrl+S): POST /api/files/write; "Saving…", then "Saved ✓" a
/// moment, then Save (off until the next change).
pub fn save(id: i32, text: &str) {
    let job = find(id, |w| {
        if !text.is_empty() { w.text = text.to_string(); }
        if w.read_only || w.file.is_empty() || w.save_label != "Save" { return None; }
        w.save_label = "Saving\u{2026}".into();
        Some(json!({ "project": w.project, "file": w.file, "content": w.text }))
    }).flatten();
    let Some(body) = job else { return };
    show();
    let content = kit::s(&body, "content");
    kit::call(move |api| api.post("/api/files/write", &body), move |r| {
        match r {
            Ok(d) if d.get("ok").is_some_and(kit::truthy) => {
                find(id, |w| { w.original = content; w.save_label = "Saved \u{2713}".into(); });
                kit::toast_ok("Saved");
                slint::Timer::single_shot(std::time::Duration::from_millis(1500), move || { find(id, |w| w.save_label = "Save".into()); show(); });
            }
            Ok(d) => { find(id, |w| w.save_label = "Save".into()); kit::toast_error(&format!("Save failed: {}", { let e = kit::s(&d, "error"); if e.is_empty() { "unknown".into() } else { e } })); }
            Err(e) => { find(id, |w| w.save_label = "Save".into()); kit::toast_error(&format!("Error: {e}")); }
        }
        show();
    });
}

fn show_snap(r: Option<Rect>) {
    g(|ws| ws.set_snap(match r { Some(r) => WorkspaceSnap { on: true, x: r.x, y: r.y, w: r.w, h: r.h }, None => WorkspaceSnap::default() }));
}

/// The Arrange bar: shown now; gone `linger` ms later (unless hovered).
fn show_arrange(linger: Option<u64>) {
    let gen = with(|s| { s.arrange = true; s.arrange_gen += 1; s.arrange_gen });
    g(|ws| ws.set_arrange(true));
    if let Some(ms) = linger {
        slint::Timer::single_shot(std::time::Duration::from_millis(ms), move || {
            if with(|s| s.arrange_gen == gen && !s.arrange_hover) { with(|s| s.arrange = false); g(|ws| ws.set_arrange(false)); }
        });
    }
}

/// The header dragged (0 press, 1 move, 2 release) at a window point.
pub fn drag(id: i32, phase: i32, px: f32, py: f32) {
    let (vw, vh) = view();
    match phase {
        0 => {
            let r = find(id, |w| { if w.max { w.max = false; } w.r });
            if let Some(r) = r { with(|s| { s.drag = Some((id, px, py, r)); s.zone = None; }); show_arrange(None); show(); }
        }
        1 => {
            let Some((did, sx, sy, r)) = with(|s| s.drag) else { return };
            if did != id { return; }
            find(id, |w| { w.r.x = r.x + px - sx; w.r.y = (r.y + py - sy).max(TOP); });
            let z = zone_at(px, py, vw, vh);
            with(|s| s.zone = z);
            show_snap(z.and_then(|z| snap_rect(z, vw, vh)));
            show();
        }
        _ => {
            let z = with(|s| { s.drag = None; s.zone.take() });
            if let Some(r) = z.and_then(|z| snap_rect(z, vw, vh)) { find(id, |w| { w.r = r; w.max = false; }); }
            show_snap(None);
            show_arrange(Some(3500));
            show();
        }
    }
}

/// A resize handle dragged: the right edge (y < 0), the bottom (x < 0), the corner.
pub fn resize(id: i32, phase: i32, px: f32, py: f32) {
    if phase == 0 {
        if let Some(r) = find(id, |w| { w.max = false; w.r }) { with(|s| s.drag = Some((id, px, py, r))); }
        front(id);
        return;
    }
    let Some((did, sx, sy, r)) = with(|s| s.drag) else { return };
    if did != id { return; }
    find(id, |w| {
        if px >= 0.0 { w.r.w = (r.w + px - sx).max(MIN_W); }
        if py >= 0.0 { w.r.h = (r.h + py - sy).max(MIN_H); }
    });
    show();
}

pub fn arrange(mode: &str) {
    let (vw, vh) = view();
    let n = with(|s| s.wins.len());
    if n == 0 { return; }
    let rects = arrange_rects(mode, n, vw, vh);
    with(|s| for (w, r) in s.wins.iter_mut().zip(rects) { w.r = r; w.max = false; });
    show_arrange(Some(3500));
    show();
}

// ─── The diff ───

/// A side of a diff row as markup: the line in its colour, its changed
/// words in white (the web marks them with a stronger background).
fn diff_markup(text: &str, words: &[(usize, usize)], colour: &str, white: &str) -> StyledText {
    let mut o = String::new();
    let mut at = 0;
    let span = |o: &mut String, s: &str, c: &str| { let e = hl::escape_md(s, 4); if !e.is_empty() { o.push_str(&format!("<font color=\"{c}\">{e}</font>")); } };
    for (a, b) in words {
        if *a > at { span(&mut o, &text[at..*a], colour); }
        span(&mut o, &text[*a..*b], white);
        at = *b;
    }
    if at < text.len() { span(&mut o, &text[at..], colour); }
    if o.is_empty() { o.push('\u{a0}'); }
    StyledText::from_markdown(&o).unwrap_or_default()
}

/// A diff in a window (feOpenDiff).
pub fn open_diff(file: &str, old: &str, new: &str) {
    let ops = diff::diff(&diff::lines(old), &diff::lines(new));
    let chunks = diff::chunks(&ops);
    let rows = diff::rows(&ops);
    let p = pal();
    let mut chunk_rows = vec![];
    let view_rows: Vec<WorkspaceDiffRow> = rows.iter().enumerate().map(|(i, r)| {
        if r.chunk >= 0 { chunk_rows.push(i); }
        let colour = |k: &str| if k == "del" { p.diff_del.clone() } else if k == "add" { p.diff_add.clone() } else { p.text.clone() };
        WorkspaceDiffRow {
            chunk: r.chunk, staged: false,
            old_ln: r.old_ln.clone().into(), old: diff_markup(&r.old, &r.old_words, &colour(r.old_kind), &p.white), old_kind: r.old_kind.into(),
            new_ln: r.new_ln.clone().into(), new: diff_markup(&r.new, &r.new_words, &colour(r.new_kind), &p.white), new_kind: r.new_kind.into(),
        }
    }).collect();
    let id = new_win("diff", file);
    find(id, |w| {
        w.project = kit::project(); w.file = file.to_string(); w.read_only = true;
        w.ops = ops; w.chunks = chunks; w.chunk_rows = chunk_rows;
        w.rows.set_vec(view_rows);
    });
    show();
}

/// ↑ / ↓: the previous or next chunk, scrolled to.
pub fn diff_step(id: i32, d: i32) {
    find(id, |w| {
        let n = w.chunks.len() as i32;
        if n == 0 { return; }
        w.cur = if d < 0 { (w.cur - 1).max(0) } else { (w.cur + 1).min(n - 1) };
        w.jump_row = *w.chunk_rows.get(w.cur as usize).unwrap_or(&0) as i32;
        w.jump_gen += 1;
    });
    show();
}

/// Stage one chunk (stageChunk): its patch to /api/git/stageHunk; staged,
/// its button says so and goes quiet.
pub fn diff_stage(id: i32, chunk: i32) {
    let job = find(id, |w| {
        let ch = w.chunks.get(chunk as usize)?.clone();
        Some((diff::chunk_patch(&w.file, &w.ops, &ch), w.project.clone()))
    }).flatten();
    let Some((patch, project)) = job else { return kit::toast_error("Invalid chunk") };
    kit::call(move |api| api.post("/api/git/stageHunk", &json!({ "project": project, "patch": patch })), move |r| match r {
        Ok(d) if d.get("ok").is_some_and(kit::truthy) => {
            kit::toast_ok(&format!("Chunk {} staged", chunk + 1));
            find(id, |w| {
                w.staged.insert(chunk as usize);
                if let Some(&ri) = w.chunk_rows.get(chunk as usize) {
                    if let Some(mut row) = w.rows.row_data(ri) { row.staged = true; w.rows.set_row_data(ri, row); }
                }
            });
            super::git::load();
        }
        Ok(d) => kit::toast_error(&format!("Stage failed: {}", { let e = kit::s(&d, "error"); if e.is_empty() { "unknown".into() } else { e } })),
        Err(e) => kit::toast_error(&format!("Error: {e}")),
    });
}

pub fn props() -> Value {
    with(|s| Value::Array(s.wins.iter().map(|w| json!({
        "id": w.id, "kind": w.kind, "title": w.title, "x": w.r.x, "y": w.r.y, "w": w.r.w, "h": w.r.h, "max": w.max,
        "dirty": w.text != w.original, "save": w.save_label, "loading": w.loading, "img": w.img.is_some(),
        "lines": w.lines.row_count(), "text": if w.kind == "edit" { w.text.clone() } else { String::new() },
        "chunks": w.chunks.len(), "staged": w.staged.len(), "counter": view_of(w).counter.as_str(), "rows": w.rows.row_count(),
    })).collect()))
}

#[allow(dead_code)]
fn _s(_: SharedString) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places() {
        let r = new_rect(0, 1400.0, 860.0);
        assert_eq!((r.w, r.y), (900.0, 54.0));
        assert!((r.h - 619.2).abs() < 0.1);
        assert_eq!(new_rect(1, 1400.0, 860.0).x, r.x + 28.0);
        assert_eq!(new_rect(6, 1400.0, 860.0).x, r.x);
        assert_eq!(zone_at(10.0, 400.0, 1400.0, 860.0), Some("left"));
        assert_eq!(zone_at(10.0, 10.0, 1400.0, 860.0), Some("tl"));
        assert_eq!(zone_at(700.0, 10.0, 1400.0, 860.0), Some("max"));
        assert_eq!(zone_at(700.0, 400.0, 1400.0, 860.0), None);
        assert_eq!(snap_rect("right", 1400.0, 860.0), Some(Rect { x: 702.0, y: 48.0, w: 694.0, h: 808.0 }));
    }

    #[test]
    fn arranges() {
        let t = arrange_rects("tile", 2, 1006.0, 852.0);
        assert_eq!(t[1].x, 3.0 + 500.0);
        let g = arrange_rects("grid", 3, 1400.0, 860.0);
        assert_eq!(g.len(), 3);
        assert_eq!(g[2].y, g[0].y + g[0].h + 6.0);
        let c = arrange_rects("cascade", 2, 1400.0, 860.0);
        assert_eq!(c[1].x - c[0].x, 28.0);
    }
}
