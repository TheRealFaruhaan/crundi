//! The Claude terminal panels (3.2; the web's mountXterm, termSendKey,
//! termSendInput and the input bar; the Slint client's terms.rs and
//! term.slint): each terminal in a pane ("term:<id>") is subscribed on /ws
//! (`subscribe`: the server sends its scrollback, then `output`), fed into
//! its emulator (vt.rs) and drawn in cells; keys typed into it, a paste, the
//! wheel and its size go back as `input` and `resize`. Its own input
//! (AREAS.md, Workspace): the text box (Ctrl+Enter or Send sends the text,
//! then Enter on its own), the key tools, attach (to crundi_attachments).

use crate::vt;
use crundi_core::attach as rules;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::{Arc, Mutex}, time::Duration};

/// An upload under way (the strip over the input).
#[derive(Default, Clone)]
struct Up { on: bool, busy: bool, name: String, label: String, frac: f32, hide_gen: u64 }

/// A drag over the terminal, by cells: from (c0, r0) to (c1, r1).
#[derive(Clone, Copy, Default)]
struct Sel { c0: usize, r0: usize, c1: usize, r1: usize, on: bool }

/// One terminal's panel.
pub struct TermView {
    pub id: String,
    term: vt::Term,
    back: usize,
    tools: bool,
    text: String,
    /// The text was set here (Send, attach): the box takes it on the next draw.
    text_dirty: bool,
    input: Option<Entity<w::TextareaState>>,
    sent: (usize, usize),
    sized: bool,
    up: Up,
    focus: FocusHandle,
    /// The screen's box in the window, and a cell's size there.
    origin: Point<Pixels>,
    cell: (f32, f32),
    pressing: bool,
    sel: Sel,
    blink_on: bool,
    blink_pending: bool,
    _subs: Vec<Subscription>,
}

#[derive(Default)]
struct Terms(BTreeMap<String, Entity<TermView>>);
impl Global for Terms {}

pub fn key_of(id: &str) -> String { format!("term:{id}") }
fn id_of(key: &str) -> String { key.strip_prefix("term:").unwrap_or(key).to_string() }

fn send(cx: &App, id: &str, data: &str) {
    if data.is_empty() { return; }
    crundi_shell::ws_send(cx, &json!({ "type": "input", "id": id, "data": data }));
}

pub fn find(cx: &App, id: &str) -> Option<Entity<TermView>> { cx.try_global::<Terms>()?.0.get(id).cloned() }

fn find_key(cx: &App, key: &str) -> Option<Entity<TermView>> { find(cx, &id_of(key)) }

/// The terminal's panel, made (and subscribed) the first time.
pub fn ensure(id: &str, cx: &mut App) -> Entity<TermView> {
    if let Some(v) = find(cx, id) { return v; }
    let v = cx.new(|cx| TermView::new(id, cx));
    if !cx.has_global::<Terms>() { cx.set_global(Terms::default()); }
    cx.global_mut::<Terms>().0.insert(id.to_string(), v.clone());
    crundi_shell::ws_send(cx, &json!({ "type": "subscribe", "id": id }));
    v
}

/// The terminals in the workbench now: the others are let go.
pub fn sync_views(cx: &mut App) {
    let Some(wb) = crundi_workbench::workbench(cx) else { return };
    let keys: Vec<String> = { let w = wb.read(cx); w.layouts.get(&w.project).map(|m| m.keys()).unwrap_or_default() };
    let ids: Vec<String> = keys.iter().filter_map(|k| k.strip_prefix("term:").map(String::from)).collect();
    let gone: Vec<String> = cx.try_global::<Terms>().map(|t| t.0.keys().filter(|k| !ids.contains(k)).cloned().collect()).unwrap_or_default();
    for id in gone {
        cx.global_mut::<Terms>().0.remove(&id);
        crundi_shell::ws_send(cx, &json!({ "type": "unsubscribe", "id": id }));
    }
}

/// The project's Claude terminals (the event stream's terminals of kind
/// "terminal"): a pane each, as the chats; panes of ones gone close.
pub fn sync_panes(cx: &mut App) {
    let Some(wb) = crundi_workbench::workbench(cx) else { return };
    let s = crundi_shell::session::get(cx);
    let ids: Vec<String> = s.terminals.iter()
        .filter(|t| t.get("kind").and_then(|k| k.as_str()) == Some("terminal") && t.get("project").and_then(|p| p.as_str()) == Some(s.project.as_str()))
        .filter_map(|t| t.get("id").and_then(|i| i.as_str()).map(String::from)).collect();
    if s.project.is_empty() { return; }
    wb.update(cx, |w, cx| {
        let keep = ids.clone();
        w.retain(move |k| !k.starts_with("term:") || keep.iter().any(|i| k == key_of(i)), cx);
        for id in &ids { let key = key_of(id); if w.mosaic().leaf_of(&key).is_none() { w.place(&key, cx); } }
    });
    sync_views(cx);
}

/// Back online: each terminal starts again from the scrollback the server
/// sends on subscribe (the web resets each first).
pub fn resubscribe(cx: &mut App) {
    let views: Vec<Entity<TermView>> = cx.try_global::<Terms>().map(|t| t.0.values().cloned().collect()).unwrap_or_default();
    for v in views {
        let (id, c, r) = v.update(cx, |t, cx| { t.term.reset(); t.back = 0; cx.notify(); (t.id.clone(), t.term.cols, t.term.rows) });
        crundi_shell::ws_send(cx, &json!({ "type": "subscribe", "id": id }));
        crundi_shell::ws_send(cx, &json!({ "type": "resize", "id": id, "cols": c, "rows": r }));
    }
}

/// The server's `output` for a terminal.
pub fn output(id: &str, data: &str, cx: &mut App) {
    let Some(v) = find(cx, id) else { return };
    let replies = v.update(cx, |t, cx| {
        let before = t.term.scrollback.len();
        t.term.feed(data);
        // Scrolled up: the view stays on the same lines as more come.
        if t.back > 0 { t.back = (t.back + t.term.scrollback.len().saturating_sub(before)).min(t.term.scrollback.len()); }
        cx.notify();
        std::mem::take(&mut t.term.replies)
    });
    for r in replies { send(cx, id, &r); }
}

// ─── Colours ───

fn rgb3(r: u8, g: u8, b: u8) -> Hsla { rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into() }

/// TERM_THEME's sixteen: black, red, green, yellow, blue, magenta, cyan,
/// white, then the bright ones.
fn named() -> [Hsla; 16] {
    [T::bg_tertiary(), T::red(), T::green(), T::yellow(), T::accent(), T::ws_term_magenta(), T::ws_term_cyan(), T::text(),
     T::text_muted(), T::ws_diff_del_text(), T::ws_diff_add_text(), T::amber(), T::accent_hover(), T::ws_term_magenta_hi(), T::ws_term_cyan_hi(), T::white()]
}

/// The xterm 256-colour palette's colour (16 named from the theme).
pub fn palette(i: u8, named: &[Hsla]) -> Hsla {
    let i = i as usize;
    if i < 16 { return named.get(i).copied().unwrap_or_default(); }
    if i < 232 {
        let lv = [0u8, 95, 135, 175, 215, 255];
        let n = i - 16;
        return rgb3(lv[n / 36], lv[(n / 6) % 6], lv[n % 6]);
    }
    let v = (8 + 10 * (i - 232)) as u8;
    rgb3(v, v, v)
}

fn resolve(c: vt::Color, fallback: Hsla, bold: bool, named: &[Hsla]) -> Hsla {
    match c {
        vt::Color::Default => fallback,
        // Bold text in the eight colours is drawn bright (xterm.js's default).
        vt::Color::Idx(i) if bold && i < 8 => palette(i + 8, named),
        vt::Color::Idx(i) => palette(i, named),
        vt::Color::Rgb(r, g, b) => rgb3(r, g, b),
    }
}

/// A run's colours as drawn (inverse swapped, dim faded): fg, bg.
fn run_colors(st: &vt::Style, named: &[Hsla]) -> (Hsla, Option<Hsla>) {
    let mut fg = resolve(st.fg, T::text(), st.bold, named);
    let bgc = if st.bg == vt::Color::Default { None } else { Some(resolve(st.bg, T::bg(), false, named)) };
    let mut bg = bgc;
    if st.inverse { let f = fg; fg = bgc.unwrap_or(T::bg()); bg = Some(f); }
    if st.dim { fg = fg.opacity(T::WS_TERM_DIM); }
    (fg, bg)
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
            let wd = vt::width(ch).max(1);
            if ch.is_ascii() { if buf.is_empty() { start = col; } buf.push(ch); col += 1; continue; }
            if !buf.is_empty() { out.push(vt::Run { col: start, cells: buf.len(), text: std::mem::take(&mut buf), st: r.st }); }
            out.push(vt::Run { col, cells: wd, text: ch.to_string(), st: r.st });
            col += wd;
        }
        if !buf.is_empty() { out.push(vt::Run { col: start, cells: buf.len(), text: buf, st: r.st }); }
    }
    out
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

/// A path put in the text box (insertIntoTermInput, at its end): a space
/// each side unless one is there.
pub fn with_path(text: &str, path: &str) -> String {
    let lead = if !text.is_empty() && !text.ends_with(char::is_whitespace) { " " } else { "" };
    format!("{text}{lead}{path} ")
}

/// A GPUI key as the emulator's key text (vt::key_bytes reads the Slint
/// client's: its private-use codes for the arrows, Home, End, F-keys…).
pub fn key_text(key: &str, key_char: Option<&str>, ctrl: bool) -> Option<String> {
    let s = match key {
        "up" => "\u{f700}", "down" => "\u{f701}", "left" => "\u{f702}", "right" => "\u{f703}",
        "home" => "\u{f729}", "end" => "\u{f72b}", "pageup" => "\u{f72c}", "pagedown" => "\u{f72d}",
        "insert" => "\u{f727}", "delete" => "\u{7f}", "backspace" => "\u{8}",
        "enter" => "\n", "tab" => "\t", "escape" => "\u{1b}", "space" => " ",
        k if k.len() >= 2 && k.starts_with('f') && k[1..].parse::<u32>().is_ok_and(|n| (1..=12).contains(&n)) => {
            let n: u32 = k[1..].parse().unwrap_or(1);
            return char::from_u32(0xf704 + n - 1).map(|c| c.to_string());
        }
        k => {
            if ctrl && k.chars().count() == 1 { return Some(k.to_string()); }
            return key_char.filter(|c| !c.is_empty()).map(String::from).or_else(|| (k.chars().count() == 1).then(|| k.to_string()));
        }
    };
    Some(s.to_string())
}

impl TermView {
    fn new(id: &str, cx: &mut Context<Self>) -> Self {
        TermView {
            id: id.to_string(), term: vt::Term::new(80, 24), back: 0, tools: false, text: String::new(), text_dirty: false, input: None,
            sent: (0, 0), sized: false, up: Up::default(), focus: cx.focus_handle(), origin: point(px(0.), px(0.)), cell: (0., 0.),
            pressing: false, sel: Sel::default(), blink_on: true, blink_pending: false, _subs: vec![],
        }
    }

    /// The panel's size in cells: the terminal and the server follow.
    fn size(&mut self, cols: usize, rows: usize, cx: &mut Context<Self>) {
        let (c, r) = (cols.max(2), rows.max(1));
        if self.sized && self.sent == (c, r) { return; }
        self.term.resize(c, r);
        self.sized = true;
        self.sent = (c, r);
        crundi_shell::ws_send(cx, &json!({ "type": "resize", "id": self.id, "cols": c, "rows": r }));
        cx.notify();
    }

    /// A key typed into the terminal: its bytes (xterm's), Shift+Enter a new
    /// line (ESC CR, as the web's default), Ctrl+V a paste. True: taken.
    pub fn key(&mut self, text: &str, ctrl: bool, alt: bool, shift: bool, cx: &mut Context<Self>) -> bool {
        if ctrl && (text == "v" || text == "V") { self.paste(cx); return true; }
        let bytes = if shift && (text == "\n" || text == "\r") { Some("\x1b\r".to_string()) } else { vt::key_bytes(text, ctrl, alt, shift, self.term.app_cursor) };
        let Some(b) = bytes else { return false };
        self.back = 0;
        send(cx, &self.id, &b);
        cx.notify();
        true
    }

    /// The clipboard's text into the terminal (bracketed when it asks).
    pub fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()).filter(|t| !t.is_empty()) else { return };
        let d = if self.term.bracketed { format!("\x1b[200~{text}\x1b[201~") } else { text };
        send(cx, &self.id, &d);
    }

    /// A drag's selection: copied to the clipboard (as xterm.js does).
    pub fn select(&mut self, c0: usize, r0: usize, c1: usize, r1: usize, cx: &mut Context<Self>) {
        let (c0, r0, c1, r1) = if (r1, c1) < (r0, c0) { (c1, r1, c0, r0) } else { (c0, r0, c1, r1) };
        let text = { let v = self.term.view(self.back); selection_text(&v, c0, r0, c1, r1) };
        if text.is_empty() { return; }
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        crate::note_copy(cx, text);
    }

    /// The wheel: up or down the scrollback; on the alternate screen (a
    /// full-screen program) the arrow keys instead.
    pub fn scroll(&mut self, lines: i32, cx: &mut Context<Self>) {
        if self.term.is_alt() {
            let app = self.term.app_cursor;
            let k = if lines > 0 { if app { "\x1bOA" } else { "\x1b[A" } } else if app { "\x1bOB" } else { "\x1b[B" };
            send(cx, &self.id, &k.repeat(lines.unsigned_abs() as usize));
            return;
        }
        let max = self.term.scrollback.len();
        self.back = if lines > 0 { (self.back + lines as usize).min(max) } else { self.back.saturating_sub(lines.unsigned_abs() as usize) };
        cx.notify();
    }

    pub fn bottom(&mut self, cx: &mut Context<Self>) { self.back = 0; cx.notify(); }

    /// The text box's text as typed (the driver's term-input sets it).
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) { self.text = text.to_string(); self.text_dirty = true; cx.notify(); }

    /// Send (and Ctrl+Enter): the text, then Enter as its own write 60ms later
    /// (an agent's interface takes a burst as a paste and would keep a
    /// trailing CR in it); the box empties.
    pub fn send_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if text.is_empty() { return; }
        send(cx, &self.id, text);
        let id = self.id.clone();
        cx.spawn(async move |_, cx: &mut AsyncApp| {
            cx.background_executor().timer(Duration::from_millis(60)).await;
            let _ = cx.update(|cx| send(cx, &id, "\r"));
        }).detach();
        self.text.clear();
        self.text_dirty = true;
        self.back = 0;
        cx.notify();
    }

    pub fn tool(&mut self, name: &str, cx: &mut Context<Self>) { if let Some(b) = tool_bytes(name) { send(cx, &self.id, b); } }

    pub fn toggle_tools(&mut self, cx: &mut Context<Self>) { self.tools = !self.tools; cx.notify(); }

    /// Attach: the system's picker, the file to /api/attachments/upload in
    /// the project (the strip shows its progress), its path into the box.
    pub fn attach(&mut self, cx: &mut Context<Self>) {
        if self.up.busy { return; }
        let project = crundi_shell::project(cx);
        if project.is_empty() { return; }
        let me = cx.entity().downgrade();
        crundi_shell::run(cx, || crundi_core::driver::take_pick().or_else(|| rfd::FileDialog::new().pick_file()), move |p, cx| {
            let Some(path) = p else { return };
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
            let bytes = match std::fs::read(&path) { Ok(b) => b, Err(e) => return crate::toast(cx, format!("Upload failed: {e}"), w::Toast::Error) };
            let Some(api) = crundi_shell::api(cx) else { return };
            let _ = me.update(cx, |t, cx| { t.up = Up { on: true, busy: true, name: name.clone(), label: "0%".into(), frac: 0.0, hide_gen: t.up.hide_gen + 1 }; cx.notify(); });
            // The strip follows the body going out.
            let prog = Arc::new(Mutex::new((0u64, 0u64, false)));
            {
                let (prog, me) = (prog.clone(), me.clone());
                cx.spawn(async move |cx: &mut AsyncApp| loop {
                    cx.background_executor().timer(Duration::from_millis(100)).await;
                    let (sent, total, done) = *prog.lock().unwrap();
                    if done { break; }
                    let _ = me.update(cx, |t, cx| { t.up.label = rules::progress_label(sent, total); t.up.frac = if total == 0 { 0.0 } else { sent as f32 / total as f32 }; cx.notify(); });
                }).detach();
            }
            let n2 = name.clone();
            crundi_shell::run(cx, move || {
                use base64::Engine;
                let body = serde_json::to_vec(&json!({ "project": project, "name": n2, "data": base64::engine::general_purpose::STANDARD.encode(&bytes) })).unwrap_or_default();
                let p2 = prog.clone();
                let r = api.post_with_progress("/api/attachments/upload", &body, &move |sent: u64, total: u64| { let mut g = p2.lock().unwrap(); g.0 = sent; g.1 = total; });
                prog.lock().unwrap().2 = true;
                r
            }, move |r, cx| {
                let answer = match r {
                    Err(crundi_core::api::ApiError::SignedOut) => return crundi_shell::session::sign_out(cx, "Your sign-in ended. Sign in again."),
                    Err(_) => rules::upload_answer(0, "", &name),
                    Ok((status, body)) => rules::upload_answer(status, &body, &name),
                };
                let g = me.update(cx, |t, cx| {
                    t.up.busy = false;
                    if let Ok((p, _)) = &answer { t.text = with_path(&t.text, p); t.text_dirty = true; }
                    cx.notify();
                    t.up.hide_gen
                }).ok();
                match answer { Ok((_, n)) => crate::toast(cx, format!("Attached: {n}"), w::Toast::Success), Err(e) => crate::toast(cx, e, w::Toast::Error) }
                // The strip stays a moment, then goes.
                if let Some(g) = g {
                    let me = me.clone();
                    cx.spawn(async move |cx: &mut AsyncApp| {
                        cx.background_executor().timer(Duration::from_millis(350)).await;
                        let _ = me.update(cx, |t, cx| if t.up.hide_gen == g && !t.up.busy { t.up.on = false; cx.notify(); });
                    }).detach();
                }
            });
        });
    }

    pub fn props(&self) -> Value {
        let (cx, cy) = self.term.cursor();
        json!({
            "id": self.id, "key": key_of(&self.id), "cols": self.term.cols, "rows": self.term.rows, "sized": self.sized,
            "screen": self.term.screen_text(), "cursor": [cx, cy], "alt": self.term.is_alt(), "back": self.back,
            "scrollback": self.term.scrollback.len(), "tools": self.tools, "text": self.text, "upload": self.up.on, "title": self.term.title,
        })
    }

    // ─── Drawing ───

    fn cell_at(&self, p: Point<Pixels>) -> (usize, usize) {
        let pad = f32::from(T::SP_4);
        let (cw, ch) = (self.cell.0.max(1.), self.cell.1.max(1.));
        let x = (f32::from(p.x - self.origin.x) - pad).max(0.);
        let y = (f32::from(p.y - self.origin.y) - pad).max(0.);
        ((x / cw) as usize, (y / ch) as usize)
    }

    fn ensure_input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<w::TextareaState> {
        if let Some(i) = &self.input { return i.clone(); }
        let st = cx.new(|cx| w::TextareaState::new(window, cx).auto_grow(1, 5).placeholder("Type here... (Ctrl+Enter or Send button)"));
        self._subs.push(cx.subscribe_in(&st, window, |this, st, ev: &w::InputEvent, _window, cx| match ev {
            w::InputEvent::Change => { this.text = st.read(cx).value().to_string(); }
            w::InputEvent::PressEnter { secondary: true, .. } => { let t = st.read(cx).value().to_string(); this.send_text(t.trim_end_matches('\n'), cx); }
            _ => {}
        }));
        self.input = Some(st.clone());
        st
    }

    /// The key tools (.term-tool-panel): Esc / Tab / Shift+Tab; the Ctrl
    /// keys and Space; Home End and the arrows round Enter.
    fn tools_view(&self, cx: &mut Context<Self>) -> Div {
        let id = self.id.clone();
        let kb = |k: &'static str, label: &'static str, cx: &mut Context<Self>| -> AnyElement {
            w::button(SharedString::from(format!("tk-{k}-{id}")), label).on_click(cx.listener(move |this, _, _, cx| this.tool(k, cx))).into_any_element()
        };
        let ki = |k: &'static str, icon: IconName, tip: &'static str, cx: &mut Context<Self>| -> AnyElement {
            w::icon_button(SharedString::from(format!("tk-{k}-{id}")), icon, tip).on_click(cx.listener(move |this, _, _, cx| this.tool(k, cx))).into_any_element()
        };
        let col = || div().flex().flex_col().gap(T::SP_4);
        let row = || div().flex().items_center().justify_center().gap(T::SP_4);
        div().mx(T::SP_8).my(T::SP_4).p(T::SP_10).rounded(T::R_CARD).border_1().border_color(T::border()).bg(T::bg_secondary())
            // Narrow, the d-pad wraps under the rest.
            .flex().flex_wrap().gap(T::SP_16)
            .child(col().child(kb("Escape", "Esc", cx)).child(kb("Tab", "Tab", cx)).child(kb("shift+tab", "Shift+Tab", cx)))
            .child(col()
                .child(row().child(kb("ctrl+c", "Ctrl+C", cx)).child(kb("ctrl+z", "Ctrl+Z", cx)))
                .child(row().child(kb("ctrl+a", "Ctrl+A", cx)).child(kb("ctrl+e", "Ctrl+E", cx)))
                .child(row().child(kb("ctrl+d", "Ctrl+D", cx)).child(kb("ctrl+l", "Ctrl+L", cx)))
                .child(kb("Space", "Space", cx)))
            .child(div().flex_1())
            .child(col()
                .child(row().child(kb("Home", "Home", cx)).child(kb("End", "End", cx)))
                .child(row().child(ki("ArrowUp", IconName::ArrowUp, "Up", cx)))
                .child(row().child(ki("ArrowLeft", IconName::ArrowLeft, "Left", cx)).child(kb("Enter", "Enter", cx)).child(ki("ArrowRight", IconName::ArrowRight, "Right", cx)))
                .child(row().child(ki("ArrowDown", IconName::ArrowDown, "Down", cx))))
    }

    fn upload_strip(&self) -> Option<Div> {
        let u = &self.up;
        if !u.on { return None; }
        let frac = u.frac.clamp(0.0, 1.0);
        Some(div().flex().items_center().gap(T::SP_8).px(T::SP_2).pb(T::SP_6).text_size(T::TEXT_11).text_color(T::text_secondary())
            .child(div().flex_1().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().child(u.name.clone()))
            .child(div().flex_1().min_w_0().child(w::meter(frac, T::accent(), T::SP_6)))
            .child(div().flex_none().font_family(T::MONO).text_color(T::text_muted()).child(u.label.clone())))
    }
}

impl Render for TermView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input = self.ensure_input(window, cx);
        if self.text_dirty {
            self.text_dirty = false;
            let t = self.text.clone();
            let n = t.len();
            input.update(cx, |st, cx| { st.set_value(t, window, cx); st.set_selected_range(n..n, cx); });
        }
        // The cell: the mono face's advance at the pane's text size.
        let fs = crundi_workbench::text_scale(cx, &self.id);
        let size = T::TEXT_13 * fs;
        // Measured on a shaped line of cells (as drawn: a glyph's advance
        // alone may be off by the renderer's rounding over a long run).
        const PROBE: &str = "MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM";
        let run = TextRun { len: PROBE.len(), font: font(T::MONO), color: T::text(), background_color: None, underline: None, strikethrough: None };
        let line = window.text_system().shape_line(PROBE.into(), size, &[run], None);
        let cw = f32::from(line.width) / PROBE.len() as f32;
        let ch = f32::from(size) * T::WS_TERM_LH;
        self.cell = (cw, ch);
        let pad = T::SP_4;
        let focused = self.focus.is_focused(window);
        // The cursor blinks while the terminal has the keys.
        if focused && !self.blink_pending {
            self.blink_pending = true;
            cx.spawn(async move |this: WeakEntity<TermView>, cx: &mut AsyncApp| {
                cx.background_executor().timer(Duration::from_millis(T::WS_TERM_BLINK_MS)).await;
                let _ = this.update(cx, |t, cx| { t.blink_pending = false; t.blink_on = !t.blink_on; cx.notify(); });
            }).detach();
        }
        if !focused { self.blink_on = true; }

        let named = named();
        let me = cx.entity().downgrade();
        let mut screen = div().id(SharedString::from(format!("term-{}", self.id))).track_focus(&self.focus).relative().flex_1().min_h_0().w_full().overflow_hidden().bg(T::bg()).cursor_text()
            .font_family(T::MONO).text_size(size).line_height(px(ch))
            .child(canvas(move |bounds, _, cx| {
                let (bw, bh) = (f32::from(bounds.size.width) - 2. * f32::from(pad), f32::from(bounds.size.height) - 2. * f32::from(pad));
                let (cols, rows) = ((bw / cw.max(1.)).floor().max(2.) as usize, (bh / ch.max(1.)).floor().max(1.) as usize);
                let _ = me.update(cx, |t, cx| { t.origin = bounds.origin; t.size(cols, rows, cx); });
            }, |_, _, _, _| {}).absolute().size_full())
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                let m = &e.keystroke.modifiers;
                let Some(text) = key_text(&e.keystroke.key, e.keystroke.key_char.as_deref(), m.control) else { return };
                if this.key(&text, m.control, m.alt, m.shift, cx) { cx.stop_propagation(); }
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| {
                this.focus.focus(window, cx);
                let (c, r) = this.cell_at(e.position);
                this.pressing = true;
                this.sel = Sel { c0: c, r0: r, c1: c, r1: r, on: false };
                cx.notify();
            }))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if !this.pressing || e.pressed_button != Some(MouseButton::Left) { return; }
                let (c, r) = this.cell_at(e.position);
                this.sel.c1 = c; this.sel.r1 = r;
                if (c, r) != (this.sel.c0, this.sel.r0) { this.sel.on = true; }
                cx.notify();
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if !this.pressing { return; }
                this.pressing = false;
                if this.sel.on { let s = this.sel; this.select(s.c0, s.r0, s.c1, s.r1, cx); }
            }))
            .on_scroll(cx.listener(move |this, e: &ScrollEvent, _, cx| {
                let y = f32::from(e.delta.pixel_delta(px(ch)).y);
                if y > 0. { this.scroll(T::WS_TERM_WHEEL, cx); } else if y < 0. { this.scroll(-T::WS_TERM_WHEEL, cx); }
                cx.stop_propagation();
            }));
        // The lines: each run at its column, in its colours.
        let view = self.term.view(self.back);
        for (r, row) in view.iter().enumerate() {
            let top = pad + px(r as f32 * ch);
            for run in split_runs(row) {
                let (fg, bg) = run_colors(&run.st, &named);
                if bg.is_none() && run.text.trim().is_empty() && !run.st.underline { continue; }
                let st = run.st;
                screen = screen.child(div().absolute().top(top).left(pad + px(run.col as f32 * cw)).w(px(run.cells as f32 * cw)).h(px(ch))
                    .whitespace_nowrap().overflow_hidden().text_color(fg)
                    .when_some(bg, |d, b| d.bg(b))
                    .when(st.bold, |d| d.font_weight(T::W_BOLD))
                    .when(st.italic, |d| d.italic())
                    .when(st.underline, |d| d.underline())
                    .child(run.text));
            }
        }
        // The selection: whole cells, row by row.
        if self.sel.on {
            let s = self.sel;
            let ((c0, r0), (c1, r1)) = if (s.r1, s.c1) < (s.r0, s.c0) { ((s.c1, s.r1), (s.c0, s.r0)) } else { ((s.c0, s.r0), (s.c1, s.r1)) };
            for r in r0..=r1 {
                let from = if r == r0 { c0 } else { 0 };
                let to = if r == r1 { c1 } else { self.term.cols.saturating_sub(1) };
                if to < from { continue; }
                screen = screen.child(div().absolute().top(pad + px(r as f32 * ch)).left(pad + px(from as f32 * cw)).w(px((to - from + 1) as f32 * cw)).h(px(ch)).bg(T::ws_term_sel()));
            }
        }
        // The cursor: a block while the terminal has the keys, a frame when not.
        if self.term.cursor_on && self.back == 0 {
            let (x, y) = self.term.cursor();
            let c = div().absolute().top(pad + px(y as f32 * ch)).left(pad + px(x as f32 * cw)).w(px(cw)).h(px(ch));
            screen = screen.child(if focused { c.when(self.blink_on, |d| d.bg(T::accent_edge())) } else { c.border_1().border_color(T::accent()) });
        }
        // Scrolled up: the jump to the bottom (.term-scroll-bottom).
        if self.back > 0 {
            screen = screen.child(div().absolute().right(T::SP_12).bottom(T::SP_12)
                .child(w::icon_button(SharedString::from(format!("term-bottom-{}", self.id)), IconName::ArrowDown, "Scroll to bottom").on_click(cx.listener(|this, _, _, cx| this.bottom(cx)))));
        }

        // The input bar (.term-input-bar): tools, the box, attach, Send.
        let id = self.id.clone();
        let bar = div().flex_none().flex().flex_col().px(T::SP_8).py(T::SP_6).bg(T::bg_secondary()).border_t_1().border_color(T::border())
            .when_some(self.upload_strip(), |d, s| d.child(s))
            .child(div().flex().items_end().gap(T::SP_6)
                .child(w::square_button(SharedString::from(format!("term-tools-{id}")), IconName::Settings, "Keyboard tools", if self.tools { w::SquareLook::On } else { w::SquareLook::Plain }, T::CONTROL, T::ICON_LG)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_tools(cx))))
                .child(div().flex_1().min_w_0().max_h(T::WS_TERM_INPUT_MAX).child(w::message_box(&input, 1.0)))
                .child(w::square_button(SharedString::from(format!("term-attach-{id}")), IconName::Paperclip, "Attach a file (uploads to crundi_attachments)", if self.up.busy { w::SquareLook::Busy } else { w::SquareLook::Plain }, T::CONTROL, T::ICON_LG)
                    .when(!self.up.busy, |d| d.on_click(cx.listener(|this, _, _, cx| this.attach(cx)))))
                .child(w::send_button(SharedString::from(format!("term-send-{id}")), "Send", 1.0).tooltip("Send to terminal")
                    .on_click(cx.listener(|this, _, _, cx| { let t = this.input.as_ref().map(|i| i.read(cx).value().to_string()).unwrap_or_default(); this.send_text(&t, cx); }))));
        div().size_full().flex().flex_col().bg(T::bg())
            .child(screen)
            .when(self.tools, |d| d.child(self.tools_view(cx)))
            .child(bar)
    }
}

/// The driver's terminal actions ("workspace:term-…"); None for another name.
pub fn act(name: &str, a: &[String], cx: &mut App) -> Option<Value> {
    let s = |i: usize| a.get(i).cloned().unwrap_or_default();
    let n = |i: usize| s(i).parse::<i64>().unwrap_or(0);
    let v = find_key(cx, &s(0));
    let ok = Some(json!({ "ok": v.is_some() }));
    let Some(v) = v else { return if name.starts_with("term-") { ok } else { None } };
    match name {
        "term-send" => { let t = s(1); v.update(cx, |x, cx| x.send_text(&t, cx)); }
        "term-input" => { let t = s(1); v.update(cx, |x, cx| x.set_text(&t, cx)); }
        "term-tool" => { let t = s(1); v.update(cx, |x, cx| x.tool(&t, cx)); }
        "term-tools" => v.update(cx, |x, cx| x.toggle_tools(cx)),
        "term-attach" => v.update(cx, |x, cx| x.attach(cx)),
        "term-bottom" => v.update(cx, |x, cx| x.bottom(cx)),
        "term-paste" => v.update(cx, |x, cx| x.paste(cx)),
        "term-select" => v.update(cx, |x, cx| x.select(n(1).max(0) as usize, n(2).max(0) as usize, n(3).max(0) as usize, n(4).max(0) as usize, cx)),
        "term-scroll" => { let l = n(1) as i32; v.update(cx, |x, cx| x.scroll(l, cx)); }
        "term-key" => {
            let (t, m) = (s(1), s(2));
            let took = v.update(cx, |x, cx| x.key(&t, m == "ctrl", m == "alt", m == "shift", cx));
            return Some(json!({ "ok": took }));
        }
        _ => return None,
    }
    ok
}

pub fn props(cx: &App) -> Value {
    Value::Array(cx.try_global::<Terms>().map(|t| t.0.values().map(|v| v.read(cx).props()).collect()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::{palette, resolve, rgb3, selection_text, split_runs, tool_bytes, with_path, key_text};
    use crate::vt;
    use gpui::Hsla;

    #[test]
    fn colours() {
        let named: Vec<Hsla> = (0..16).map(|i| rgb3(i, i, i)).collect();
        assert_eq!(palette(3, &named), rgb3(3, 3, 3));
        assert_eq!(palette(16, &named), rgb3(0, 0, 0));
        assert_eq!(palette(231, &named), rgb3(255, 255, 255));
        assert_eq!(palette(232, &named), rgb3(8, 8, 8));
        assert_eq!(resolve(vt::Color::Idx(1), Hsla::default(), true, &named), rgb3(9, 9, 9));
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

    #[test]
    fn gpui_keys() {
        assert_eq!(key_text("up", None, false).as_deref(), Some("\u{f700}"));
        assert_eq!(key_text("enter", None, false).as_deref(), Some("\n"));
        assert_eq!(key_text("a", Some("A"), false).as_deref(), Some("A"));
        assert_eq!(key_text("c", None, true).as_deref(), Some("c"));
        assert_eq!(key_text("f1", None, false).as_deref(), Some("\u{f704}"));
        assert_eq!(key_text("shift", None, false), None);
        assert_eq!(vt::key_bytes(&key_text("c", None, true).unwrap(), true, false, false, false).as_deref(), Some("\x03"));
    }
}
