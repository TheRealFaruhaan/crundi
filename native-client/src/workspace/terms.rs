//! The Claude terminal panels (3.2; the web's mountXterm, termSendKey,
//! termSendInput and the input bar): each terminal in a pane is subscribed
//! on /ws (`subscribe`, the server sends its scrollback, then `output`),
//! fed into its emulator (vt.rs) and drawn; keys typed into it, a paste,
//! the wheel and a resize go back as `input` and `resize`. Its own input
//! (AREAS.md, Workspace): the text box (Ctrl+Enter or Send sends the text,
//! then Enter on its own), the key tools, attach (to crundi_attachments).

use super::{g, pal, vt};
use crate::{kit, MainWindow, WorkspaceRun, WorkspaceTerm, WorkspaceTermLine};
use serde_json::{json, Value};
use slint::{Color, ComponentHandle, Model, ModelRc, VecModel};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[derive(Default, Clone)]
struct Up { on: bool, busy: bool, name: String, label: String, frac: f32, hide_gen: u64 }

struct T {
    id: String,
    term: vt::Term,
    back: usize,
    tools: bool,
    text: String,
    text_gen: i32,
    lines: Rc<VecModel<WorkspaceTermLine>>,
    drawn: Vec<Vec<vt::Run>>,
    sent: (usize, usize),
    sized: bool,
    up: Up,
}

#[derive(Default)]
struct St {
    terms: BTreeMap<String, T>,
    model: Rc<VecModel<WorkspaceTerm>>,
    queued: bool,
}

thread_local! { static ST: RefCell<St> = RefCell::new(St::default()); }

fn with<R>(f: impl FnOnce(&mut St) -> R) -> R { ST.with(|s| f(&mut s.borrow_mut())) }

fn key_of(id: &str) -> String { crate::app::term_key(id) }
fn id_of(key: &str) -> String { crate::app::term_of(key).unwrap_or(key).to_string() }

fn send(id: &str, data: &str) {
    if data.is_empty() { return; }
    kit::ws_send(&json!({ "type": "input", "id": id, "data": data }));
}

/// The xterm 256-colour palette's colour (16 named from the theme).
pub fn palette(i: u8, named: &[Color]) -> Color {
    let i = i as usize;
    if i < 16 { return named.get(i).copied().unwrap_or_default(); }
    if i < 232 {
        let lv = [0u8, 95, 135, 175, 215, 255];
        let n = i - 16;
        return Color::from_rgb_u8(lv[n / 36], lv[(n / 6) % 6], lv[n % 6]);
    }
    let v = (8 + 10 * (i - 232)) as u8;
    Color::from_rgb_u8(v, v, v)
}

fn resolve(c: vt::Color, fallback: Color, bold: bool, named: &[Color]) -> Color {
    match c {
        vt::Color::Default => fallback,
        // Bold text in the eight colours is drawn bright (xterm.js's default).
        vt::Color::Idx(i) if bold && i < 8 => palette(i + 8, named),
        vt::Color::Idx(i) => palette(i, named),
        vt::Color::Rgb(r, g, b) => Color::from_rgb_u8(r, g, b),
    }
}

/// A run as drawn: its colours resolved (inverse swapped, dim faded).
fn run_view(r: &vt::Run, p: &super::Palette) -> WorkspaceRun {
    let st = r.st;
    let mut fg = resolve(st.fg, p.fg, st.bold, &p.term);
    let bgc = if st.bg == vt::Color::Default { None } else { Some(resolve(st.bg, p.bg, false, &p.term)) };
    let mut bg = bgc;
    if st.inverse { let f = fg; fg = bgc.unwrap_or(p.bg); bg = Some(f); }
    if st.dim { fg = fg.with_alpha(0.5); }
    WorkspaceRun { col: r.col as i32, n: r.cells as i32, text: r.text.clone().into(), fg, bg: bg.unwrap_or_default(), has_bg: bg.is_some(), bold: st.bold, italic: st.italic, underline: st.underline }
}

/// A line's runs, each character past ASCII on its own (a font's fallback
/// glyph may not be a cell wide: each lands on its own column).
fn split_runs(row: &[vt::Cell]) -> Vec<vt::Run> {
    let mut out = vec![];
    for r in vt::runs(row) {
        if r.text.is_ascii() { out.push(r); continue; }
        let mut col = r.col;
        let mut buf = String::new();
        let mut start = col;
        for ch in r.text.chars() {
            let w = vt::width(ch).max(1);
            if ch.is_ascii() { if buf.is_empty() { start = col; } buf.push(ch); col += 1; continue; }
            if !buf.is_empty() { out.push(vt::Run { col: start, cells: buf.len(), text: std::mem::take(&mut buf), st: r.st }); }
            out.push(vt::Run { col, cells: w, text: ch.to_string(), st: r.st });
            col += w;
        }
        if !buf.is_empty() { out.push(vt::Run { col: start, cells: buf.len(), text: buf, st: r.st }); }
    }
    out
}

pub fn init(w: &MainWindow) {
    let ws = w.global::<crate::WorkspaceUi>();
    with(|s| ws.set_terms(ModelRc::from(s.model.clone())));
    ws.on_term_size(|k, c, r| size(&k, c, r));
    ws.on_term_key(|k, t, c, a, sh| key(&k, &t, c, a, sh));
    ws.on_term_send(|k, t| send_text(&k, &t));
    ws.on_term_text(|k, t| input(&k, &t));
    ws.on_term_tool(|k, n| tool(&k, &n));
    ws.on_term_tools(|k| toggle_tools(&k));
    ws.on_term_attach(|k| attach(&k));
    ws.on_term_scroll(|k, n| scroll(&k, n));
    ws.on_term_bottom(|k| bottom(&k));
    ws.on_term_paste(|k| paste(&k));
    ws.on_term_select(|k, c0, r0, c1, r1| select(&k, c0, r0, c1, r1));
}

fn new_t(id: &str) -> T {
    T { id: id.to_string(), term: vt::Term::new(80, 24), back: 0, tools: false, text: String::new(), text_gen: 0, lines: Rc::new(VecModel::default()), drawn: vec![], sent: (0, 0), sized: false, up: Up::default() }
}

/// The panes now: new terminals subscribed, gone ones let go.
pub fn sync(ids: &[String]) {
    let (new, gone) = with(|s| {
        let gone: Vec<String> = s.terms.keys().filter(|k| !ids.contains(k)).cloned().collect();
        for k in &gone { s.terms.remove(k); }
        let new: Vec<String> = ids.iter().filter(|i| !s.terms.contains_key(*i)).cloned().collect();
        for id in &new { s.terms.insert(id.clone(), new_t(id)); }
        (new, gone)
    });
    for id in &gone { kit::ws_send(&json!({ "type": "unsubscribe", "id": id })); }
    for id in &new { kit::ws_send(&json!({ "type": "subscribe", "id": id })); }
    if !new.is_empty() || !gone.is_empty() { render(); }
}

pub fn clear() {
    let ids: Vec<String> = with(|s| { let k = s.terms.keys().cloned().collect(); s.terms.clear(); k });
    for id in ids { kit::ws_send(&json!({ "type": "unsubscribe", "id": id })); }
    render();
}

/// Back online: each terminal starts again from the scrollback the server
/// sends on subscribe (the web resets each first).
pub fn resubscribe() {
    let ids: Vec<(String, (usize, usize))> = with(|s| s.terms.values_mut().map(|t| { t.term.reset(); t.back = 0; (t.id.clone(), (t.term.cols, t.term.rows)) }).collect());
    for (id, (c, r)) in ids {
        kit::ws_send(&json!({ "type": "subscribe", "id": id }));
        kit::ws_send(&json!({ "type": "resize", "id": id, "cols": c, "rows": r }));
    }
    render();
}

/// The server's `output` for a terminal.
pub fn output(id: &str, data: &str) {
    let replies = with(|s| {
        let t = s.terms.get_mut(id)?;
        let before = t.term.scrollback.len();
        t.term.feed(data);
        // Scrolled up: the view stays on the same lines as more come.
        if t.back > 0 { t.back = (t.back + t.term.scrollback.len().saturating_sub(before)).min(t.term.scrollback.len()); }
        Some(std::mem::take(&mut t.term.replies))
    });
    if let Some(r) = replies { for x in r { send(id, &x); } }
    queue();
}

/// Draw on the next frame (output comes in bursts).
fn queue() {
    if with(|s| std::mem::replace(&mut s.queued, true)) { return; }
    slint::Timer::single_shot(std::time::Duration::from_millis(16), || { with(|s| s.queued = false); render(); });
}

fn render() {
    let p = pal();
    let rows: Vec<WorkspaceTerm> = with(|s| s.terms.values_mut().map(|t| {
        let view: Vec<Vec<vt::Run>> = t.term.view(t.back).iter().map(|r| split_runs(r)).collect();
        for (i, runs) in view.iter().enumerate() {
            if t.drawn.get(i) == Some(runs) { continue; }
            let line = WorkspaceTermLine { runs: ModelRc::new(VecModel::from(runs.iter().map(|r| run_view(r, &p)).collect::<Vec<_>>())) };
            if i < t.lines.row_count() { t.lines.set_row_data(i, line); } else { t.lines.push(line); }
        }
        while t.lines.row_count() > view.len() { t.lines.remove(t.lines.row_count() - 1); }
        t.drawn = view;
        let (cx, cy) = t.term.cursor();
        WorkspaceTerm {
            key: key_of(&t.id).into(), lines: ModelRc::from(t.lines.clone()),
            cursor_col: cx as i32, cursor_row: cy as i32, cursor_on: t.term.cursor_on, back: t.back > 0,
            tools: t.tools, text: t.text.clone().into(), text_gen: t.text_gen,
            up_on: t.up.on, up_name: t.up.name.clone().into(), up_label: t.up.label.clone().into(), up_frac: t.up.frac, up_busy: t.up.busy,
            exited: false,
        }
    }).collect());
    with(|s| {
        let m = &s.model;
        for (i, r) in rows.iter().enumerate() {
            if i < m.row_count() { if m.row_data(i).as_ref() != Some(r) { m.set_row_data(i, r.clone()); } } else { m.push(r.clone()); }
        }
        while m.row_count() > rows.len() { m.remove(m.row_count() - 1); }
    });
}

/// The panel's size in cells: the terminal and the server follow.
pub fn size(key: &str, cols: i32, rows: i32) {
    let id = id_of(key);
    let send_it = with(|s| {
        let t = s.terms.get_mut(&id)?;
        let (c, r) = (cols.max(2) as usize, rows.max(1) as usize);
        t.term.resize(c, r);
        t.sized = true;
        if t.sent == (c, r) { return None; }
        t.sent = (c, r);
        Some((c, r))
    });
    if let Some((c, r)) = send_it { kit::ws_send(&json!({ "type": "resize", "id": id, "cols": c, "rows": r })); }
    queue();
}

/// A key typed into the terminal: its bytes (xterm's), Shift+Enter a new
/// line (ESC CR, as the web's default), Ctrl+V a paste. True: taken.
pub fn key(key: &str, text: &str, ctrl: bool, alt: bool, shift: bool) -> bool {
    let id = id_of(key);
    if ctrl && (text == "v" || text == "V") { paste(key); return true; }
    let app_cursor = with(|s| s.terms.get(&id).map(|t| t.term.app_cursor)).unwrap_or(false);
    let bytes = if shift && (text == "\n" || text == "\r") { Some("\x1b\r".to_string()) } else { vt::key_bytes(text, ctrl, alt, shift, app_cursor) };
    let Some(b) = bytes else { return false };
    with(|s| if let Some(t) = s.terms.get_mut(&id) { t.back = 0; });
    send(&id, &b);
    queue();
    true
}

/// The text of a selection on the lines in view (from c0,r0 to c1,r1 in
/// order), each line's trailing spaces dropped.
pub fn selection_text(lines: &[&Vec<vt::Cell>], c0: usize, r0: usize, c1: usize, r1: usize) -> String {
    let mut out = vec![];
    for r in r0..=r1.min(lines.len().saturating_sub(1)) {
        let row = lines[r];
        let from = if r == r0 { c0 } else { 0 };
        let to = if r == r1 { (c1 + 1).min(row.len()) } else { row.len() };
        let s: String = row.get(from.min(row.len())..to.max(from).min(row.len())).unwrap_or(&[]).iter().filter(|c| !c.cont).map(|c| c.ch).collect();
        out.push(s.trim_end().to_string());
    }
    out.join("\n")
}

/// A drag's selection: copied to the clipboard (as xterm.js does).
pub fn select(key: &str, c0: i32, r0: i32, c1: i32, r1: i32) {
    let id = id_of(key);
    let text = with(|s| s.terms.get(&id).map(|t| {
        let v = t.term.view(t.back);
        selection_text(&v, c0.max(0) as usize, r0.max(0) as usize, c1.max(0) as usize, r1.max(0) as usize)
    })).unwrap_or_default();
    if text.is_empty() { return; }
    super::note_copy(&text);
    let _ = arboard::Clipboard::new().and_then(|mut c| c.set_text(text));
}

/// The clipboard's text into the terminal (bracketed when it asks).
pub fn paste(key: &str) {
    let id = id_of(key);
    let Some(text) = arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok()).filter(|t| !t.is_empty()) else { return };
    let br = with(|s| s.terms.get(&id).map(|t| t.term.bracketed)).unwrap_or(false);
    send(&id, &if br { format!("\x1b[200~{text}\x1b[201~") } else { text });
}

/// The wheel: up or down the scrollback; on the alternate screen (a
/// full-screen program) the arrow keys instead.
pub fn scroll(key: &str, lines: i32) {
    let id = id_of(key);
    let keys = with(|s| {
        let t = s.terms.get_mut(&id)?;
        if t.term.is_alt() {
            let k = if lines > 0 { if t.term.app_cursor { "\x1bOA" } else { "\x1b[A" } } else if t.term.app_cursor { "\x1bOB" } else { "\x1b[B" };
            return Some(k.repeat(lines.unsigned_abs() as usize));
        }
        let max = t.term.scrollback.len();
        t.back = if lines > 0 { (t.back + lines as usize).min(max) } else { t.back.saturating_sub(lines.unsigned_abs() as usize) };
        None
    });
    if let Some(k) = keys { send(&id, &k); }
    render();
}

pub fn bottom(key: &str) { let id = id_of(key); with(|s| if let Some(t) = s.terms.get_mut(&id) { t.back = 0; }); render(); }

/// The text box's text as typed.
pub fn input(key: &str, text: &str) { let id = id_of(key); with(|s| if let Some(t) = s.terms.get_mut(&id) { t.text = text.to_string(); }); }

/// Send (and Ctrl+Enter): the text, then Enter as its own write 60ms later
/// (an agent's interface takes a burst as a paste and would keep a trailing
/// CR in it); the box empties.
pub fn send_text(key: &str, text: &str) {
    let id = id_of(key);
    if text.is_empty() { return; }
    send(&id, text);
    let id2 = id.clone();
    slint::Timer::single_shot(std::time::Duration::from_millis(60), move || send(&id2, "\r"));
    with(|s| if let Some(t) = s.terms.get_mut(&id) { t.text.clear(); t.text_gen += 1; t.back = 0; });
    render();
}

/// A key of the tools (termSendKey's map).
pub fn tool_bytes(name: &str) -> Option<&'static str> {
    Some(match name {
        "Escape" => "\x1b", "Tab" => "\t", "shift+tab" => "\x1b[Z",
        "ArrowUp" => "\x1b[A", "ArrowDown" => "\x1b[B", "ArrowRight" => "\x1b[C", "ArrowLeft" => "\x1b[D",
        "Home" => "\x1b[H", "End" => "\x1b[F",
        "ctrl+c" => "\x03", "ctrl+z" => "\x1a", "ctrl+a" => "\x01", "ctrl+e" => "\x05", "ctrl+l" => "\x0c", "ctrl+d" => "\x04",
        "Enter" => "\r", "Space" => " ",
        _ => return None,
    })
}

pub fn tool(key: &str, name: &str) { if let Some(b) = tool_bytes(name) { send(&id_of(key), b); } }

pub fn toggle_tools(key: &str) { let id = id_of(key); with(|s| if let Some(t) = s.terms.get_mut(&id) { t.tools = !t.tools; }); render(); }

/// A path put in the text box (insertIntoTermInput, at its end): a space
/// each side unless one is there.
pub fn with_path(text: &str, path: &str) -> String {
    let lead = if !text.is_empty() && !text.ends_with(char::is_whitespace) { " " } else { "" };
    format!("{text}{lead}{path} ")
}

/// Attach: the system's picker, the file to /api/attachments/upload in
/// the project (the strip shows its progress), its path into the box.
pub fn attach(key: &str) {
    let id = id_of(key);
    if with(|s| s.terms.get(&id).is_none_or(|t| t.up.busy)) { return; }
    let project = kit::project();
    if project.is_empty() { return; }
    let id2 = id.clone();
    kit::bg(|| crate::testctl::take_pick().or_else(|| rfd::FileDialog::new().pick_file()), move |p| {
        let Some(path) = p else { return };
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
        let Ok(bytes) = std::fs::read(&path) else { return kit::toast_error("Upload failed") };
        let Some(api) = kit::api() else { return };
        with(|s| if let Some(t) = s.terms.get_mut(&id2) { t.up = Up { on: true, busy: true, name: name.clone(), label: "0%".into(), frac: 0.0, hide_gen: t.up.hide_gen + 1 }; });
        render();
        let (id3, n3) = (id2.clone(), name.clone());
        kit::bg(move || {
            use base64::Engine;
            let body = serde_json::to_vec(&json!({ "project": project, "name": n3, "data": base64::engine::general_purpose::STANDARD.encode(&bytes) })).unwrap_or_default();
            let idp = id3.clone();
            let progress = move |sent: u64, total: u64| {
                let idq = idp.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    with(|s| if let Some(t) = s.terms.get_mut(&idq) { t.up.label = crate::attach::progress_label(sent, total); t.up.frac = if total == 0 { 0.0 } else { sent as f32 / total as f32 }; });
                    queue();
                });
            };
            api.post_with_progress("/api/attachments/upload", &body, &progress)
        }, move |r| {
            let answer = match r { Ok((st, body)) => crate::attach::upload_answer(st, &body, &name), Err(_) => crate::attach::upload_answer(0, "", &name) };
            let gen = with(|s| s.terms.get_mut(&id2).map(|t| {
                t.up.busy = false;
                if let Ok((p, _)) = &answer { t.text = with_path(&t.text, p); t.text_gen += 1; }
                t.up.hide_gen
            }));
            match answer { Ok((_, n)) => kit::toast(&format!("Attached: {n}")), Err(e) => kit::toast_error(&e) }
            render();
            if let Some(gen) = gen {
                let id4 = id2.clone();
                slint::Timer::single_shot(std::time::Duration::from_millis(350), move || {
                    with(|s| if let Some(t) = s.terms.get_mut(&id4) { if t.up.hide_gen == gen && !t.up.busy { t.up.on = false; } });
                    render();
                });
            }
        });
    });
}

pub fn props() -> Value {
    with(|s| Value::Array(s.terms.values().map(|t| {
        let (cx, cy) = t.term.cursor();
        json!({
            "id": t.id, "key": key_of(&t.id), "cols": t.term.cols, "rows": t.term.rows, "sized": t.sized,
            "screen": t.term.screen_text(), "cursor": [cx, cy], "alt": t.term.is_alt(), "back": t.back,
            "scrollback": t.term.scrollback.len(), "tools": t.tools, "text": t.text, "upload": t.up.on, "title": t.term.title,
        })
    }).collect()))
}

#[allow(dead_code)]
fn _g() { let _ = g(|_| ()); }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours() {
        let named: Vec<Color> = (0..16).map(|i| Color::from_rgb_u8(i, i, i)).collect();
        assert_eq!(palette(3, &named), Color::from_rgb_u8(3, 3, 3));
        assert_eq!(palette(16, &named), Color::from_rgb_u8(0, 0, 0));
        assert_eq!(palette(231, &named), Color::from_rgb_u8(255, 255, 255));
        assert_eq!(palette(232, &named), Color::from_rgb_u8(8, 8, 8));
        assert_eq!(resolve(vt::Color::Idx(1), Color::default(), true, &named), Color::from_rgb_u8(9, 9, 9));
    }

    #[test]
    fn splits_wide() {
        let mut t = vt::Term::new(10, 1);
        t.feed("ab●cd");
        let r = split_runs(&t.view(0)[0]);
        assert_eq!(r.iter().map(|r| (r.col, r.text.as_str())).collect::<Vec<_>>(), vec![(0, "ab"), (2, "●"), (3, "cd")]);
    }

    #[test]
    fn selections() {
        let mut t = vt::Term::new(10, 3);
        t.feed("hello\r\nworld  x\r\nend");
        let v = t.view(0);
        assert_eq!(selection_text(&v, 1, 0, 2, 0), "el");
        assert_eq!(selection_text(&v, 3, 0, 4, 1), "lo\nworld");
        assert_eq!(selection_text(&v, 0, 1, 9, 2), "world  x\nend");
    }

    #[test]
    fn input_bits() {
        assert_eq!(with_path("", "/a/b.png"), "/a/b.png ");
        assert_eq!(with_path("see", "/a"), "see /a ");
        assert_eq!(with_path("see ", "/a"), "see /a ");
        assert_eq!(tool_bytes("ctrl+c"), Some("\x03"));
        assert_eq!(tool_bytes("nope"), None);
    }
}
