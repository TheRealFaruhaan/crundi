//! 2.22: the resume picker (#chat-resume-modal; specs/resume-picker.md):
//! before a launcher starts Claude, which conversation: a new one or an
//! earlier one of the project (Claude Code's transcripts on the server). A
//! heavy one (UI Mode) offers compacting first.
//!
//! The launcher is the workbench's (Workspace's): it calls `choose(…)` with
//! what to do with the answer (the create call, its busy look, the chat
//! taking its leaf). This module fetches the list, shows the picker, and
//! answers once: New session, Resume or Compact (with the conversation's
//! id), or nothing (cancelled). An empty list, or a server without the list
//! (404), answers New session at once, with no dialog.

use crate::LogView;
use crundi_core::{api::{ApiError, ApiResult}, resume::{self, Earlier}};
use crundi_theme::T;
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Map, Value};
use std::rc::Rc;

/// The answer: "new" | "resume" | "compact", and the conversation (resume, compact).
#[derive(Clone, Debug, PartialEq)]
pub struct Choice { pub mode: &'static str, pub resume_id: String }

type Done = Rc<dyn Fn(Choice, &mut Window, &mut App)>;

// Tab and Shift+Tab inside the picker: its own bindings (its key context
// outranks the window's focus navigation, which would take the keyboard
// out of the dialog).
actions!(resume_picker, [PickerTab, PickerBacktab]);

/// The picker's key bindings (called once, from the log's init).
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("tab", PickerTab, Some("ResumePicker")), KeyBinding::new("shift-tab", PickerBacktab, Some("ResumePicker"))]);
}

#[derive(Default)]
pub struct State {}

/// This feature's fields in the chat's props (none: the picker is the window's).
pub fn props(_v: &LogView, _cx: &App, _o: &mut Map<String, Value>) {}

/// This feature's driver actions on a chat (none).
pub fn act(_name: &str, _args: &[String], _v: &mut LogView, _window: &mut Window, _cx: &mut Context<LogView>) -> Option<Value> { None }

/// The picker while it is open.
struct Dlg {
    skip: bool,
    /// Terminal mode (3.2): a heavy one resumes directly (the CLI asks its own question).
    term: bool,
    list: Vec<Earlier>,
    error: String,
    /// The heavy choice is showing, for this row.
    heavy: Option<usize>,
    /// The keyboard: 0 New session, 1..=n the rows, n+1 Cancel (in the heavy
    /// choice: 0 Compact, 1 Load the full conversation, 2 Back).
    focus: usize,
    done: Done,
}

pub struct Picker {
    dlg: Option<Dlg>,
    /// The list is being fetched (more clicks are ignored).
    loading: bool,
    scroll: ScrollHandle,
    focus: FocusHandle,
    /// The last answer (the driver's view, until the launcher shows it).
    last: Option<Choice>,
}

struct PickerHandle(Entity<Picker>);
impl Global for PickerHandle {}

fn picker(cx: &mut App) -> Entity<Picker> {
    if let Some(h) = cx.try_global::<PickerHandle>() { return h.0.clone(); }
    let p = cx.new(|cx| Picker { dlg: None, loading: false, scroll: ScrollHandle::new(), focus: cx.focus_handle(), last: None });
    cx.set_global(PickerHandle(p.clone()));
    p
}

fn stub_result(v: Value) -> ApiResult {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    if let Some(ms) = v.get("delayMs").and_then(|x| x.as_u64()) { std::thread::sleep(std::time::Duration::from_millis(ms)); }
    match v.get("status").and_then(|x| x.as_u64()) {
        Some(st) if st >= 400 => Err(ApiError::Status(st as u16, s("error"))),
        _ if v.get("net").is_some() => Err(ApiError::Net(s("net"))),
        _ => Ok(v),
    }
}

fn now_secs() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn enc(v: &str) -> String {
    v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// What the fetch gave: the rows, or the error line's words ("" for none).
/// A 404 (an old server without the list) reads as an empty list.
fn outcome(r: Result<Value, String>, now: f64) -> (Vec<Earlier>, String) {
    match r {
        Ok(v) => (resume::rows(&v, now), String::new()),
        Err(e) if e == ApiError::Status(404, String::new()).to_string() => (vec![], String::new()),
        Err(e) => (vec![], resume::list_error(&e)),
    }
}

/// A launcher's UI Mode (or Terminal): which conversation first. Returns
/// false when ignored (the list is loading, or the picker is open).
/// `done` gets the answer; never called when the person cancels.
pub fn choose(project: &str, skip: bool, term: bool, cx: &mut App, done: impl Fn(Choice, &mut Window, &mut App) + 'static) -> bool {
    if project.is_empty() { return false; }
    let p = picker(cx);
    if p.read(cx).loading || p.read(cx).dlg.is_some() { return false; }
    p.update(cx, |p, _| p.loading = true);
    let path = format!("/api/ui-sessions/resumable?project={}", enc(project));
    let stub = crundi_core::driver::take_stub("resumable");
    let done: Done = Rc::new(done);
    crundi_shell::call(cx, move |api| match stub { Some(v) => stub_result(v), None => api.get(&path) }, move |r, cx| {
        let p = picker(cx);
        p.update(cx, |p, _| p.loading = false);
        let (list, error) = outcome(r, now_secs());
        let Some(win) = cx.active_window().or_else(|| cx.windows().first().copied()) else { return };
        let _ = win.update(cx, |_, window, cx| {
            if list.is_empty() && error.is_empty() {
                let c = Choice { mode: "new", resume_id: String::new() };
                p.update(cx, |p, _| p.last = Some(c.clone()));
                return done(c, window, cx);
            }
            p.update(cx, |p, cx| { p.dlg = Some(Dlg { skip, term, list, error, heavy: None, focus: 0, done }); p.scroll.set_offset(point(px(0.), px(0.))); cx.notify(); });
            let fh = p.read(cx).focus.clone();
            let pc = p.clone();
            w::modal(window, cx, T::DIALOG_W_XL, p.clone().into(), move |_, cx| pc.update(cx, |p, cx| { p.dlg = None; cx.notify(); }));
            window.focus(&fh, cx);
        });
    });
    true
}

impl Picker {
    fn rows_n(&self) -> usize { self.dlg.as_ref().map(|d| if d.heavy.is_some() { 0 } else { d.list.len() }).unwrap_or(0) }

    /// The last focusable place.
    fn last_focus(&self) -> usize {
        match &self.dlg { Some(d) if d.heavy.is_some() => 2, Some(d) => d.list.len() + 1, None => 0 }
    }

    /// An answer: close and hand it to the launcher.
    fn answer(&mut self, c: Choice, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.dlg.take() else { return };
        self.last = Some(c.clone());
        w::close_dialog(window, cx);
        cx.notify();
        (d.done)(c, window, cx);
    }

    fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) { self.answer(Choice { mode: "new", resume_id: String::new() }, window, cx); }

    /// A row picked: a heavy one in UI Mode offers compacting first.
    fn pick(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.dlg.as_mut() else { return };
        let Some(e) = d.list.get(i) else { return };
        if e.heavy && !d.term { d.heavy = Some(i); d.focus = 0; cx.notify(); return; }
        let id = e.id.clone();
        self.answer(Choice { mode: "resume", resume_id: id }, window, cx);
    }

    fn heavy(&mut self, mode: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.dlg.as_ref().and_then(|d| d.heavy.and_then(|i| d.list.get(i)).map(|e| e.id.clone())) else { return };
        self.answer(Choice { mode, resume_id: id }, window, cx);
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        // ← Back: the list again (the rows only: an error line can't be there).
        if let Some(d) = self.dlg.as_mut() { if let Some(i) = d.heavy.take() { d.focus = i + 1; } }
        self.follow();
        cx.notify();
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dlg.take().is_some() { w::close_dialog(window, cx); }
        cx.notify();
    }

    /// Press the focused place (Enter, Space).
    fn press(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.dlg.as_ref() else { return };
        let f = d.focus;
        if d.heavy.is_some() {
            match f { 0 => self.heavy("compact", window, cx), 1 => self.heavy("resume", window, cx), _ => self.back(cx) }
        } else if f == 0 { self.new_session(window, cx) }
        else if f <= d.list.len() { self.pick(f - 1, window, cx) }
        else { self.cancel(window, cx) }
    }

    /// The keyboard moves: ↓/↑ (stopping at the ends), Tab/Shift+Tab (round).
    fn step(&mut self, by: i32, wrap: bool, cx: &mut Context<Self>) {
        let last = self.last_focus() as i32;
        let Some(d) = self.dlg.as_mut() else { return };
        let mut f = d.focus as i32 + by;
        // ↓/↑ walk New session and the rows (Cancel is Tab's).
        let top = if wrap { last } else if d.heavy.is_some() { 2 } else { last - 1 };
        if wrap { if f > last { f = 0 } else if f < 0 { f = last } } else { f = f.clamp(0, top) }
        d.focus = f as usize;
        self.follow();
        cx.notify();
    }

    /// Keep the focused row in view (the list's item index = the focus).
    fn follow(&self) {
        if let Some(d) = &self.dlg { if d.heavy.is_none() && d.focus <= d.list.len() { self.scroll.scroll_to_item(d.focus); } }
    }

    fn key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let shift = ev.keystroke.modifiers.shift;
        match ev.keystroke.key.as_str() {
            "down" => self.step(1, false, cx),
            "up" => self.step(-1, false, cx),
            "tab" => self.step(if shift { -1 } else { 1 }, true, cx),
            "enter" | "space" => self.press(window, cx),
            "escape" => self.cancel(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    /// The driver's view of the picker (the Slint client's names).
    fn props(&self) -> Value {
        let Some(d) = &self.dlg else { return Value::Null };
        let heavy = d.heavy.and_then(|i| d.list.get(i));
        let rows: Vec<Value> = if d.heavy.is_some() { vec![] } else { d.list.iter().map(|e| json!({ "title": e.title, "meta": e.meta, "heavy": e.heavy, "inUse": e.in_use })).collect() };
        let view = self.scroll.bounds();
        let max = self.scroll.max_offset();
        let off = -f32::from(self.scroll.offset().y);
        let first_top = self.scroll.bounds_for_item(0).map(|b| f32::from(b.top())).unwrap_or(f32::from(view.top()));
        let focus_bottom = if d.heavy.is_none() { self.scroll.bounds_for_item(d.focus).map(|b| f32::from(b.bottom()) - first_top).unwrap_or(0.) } else { 0. };
        json!({
            "heading": heading(d.skip, d.term), "error": d.error, "heavy": d.heavy.is_some(),
            "heavyTitle": heavy.map(|e| e.title.clone()).unwrap_or_default(),
            "heavyIntro": heavy.map(|e| resume::heavy_intro(e.tokens)).unwrap_or_default(),
            "rows": rows, "focus": d.focus, "scroll": off, "viewH": f32::from(view.size.height),
            "contentH": f32::from(view.size.height) + f32::from(max.y), "focusBottom": focus_bottom,
        })
    }
}

fn heading(skip: bool, term: bool) -> String {
    if term { if skip { "Open Terminal — Skip Permissions".into() } else { "Open Terminal".into() } } else { resume::heading(skip) }
}

/// One choice row (New session, an earlier conversation, the heavy choice's).
fn row(id: SharedString, focused: bool, accent: bool, title: String, meta: Option<AnyElement>, on: impl Fn(&mut Picker, &mut Window, &mut Context<Picker>) + 'static, cx: &mut Context<Picker>) -> AnyElement {
    div().id(id).w_full().flex().flex_col().gap(T::SP_2).px(T::SP_12).py(T::SP_8).rounded(T::R_CONTROL).cursor_pointer()
        .bg(T::bg()).border_1().border_color(if accent { T::accent() } else { T::border() })
        .when(focused, |d| d.border_color(T::accent_hover()).bg(T::accent_dim()))
        .hover(|d| d.border_color(T::accent()).bg(T::accent_dim()))
        .child(div().text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD).text_color(if accent { T::accent_hover() } else { T::text() }).child(title))
        .children(meta)
        .on_click(cx.listener(move |p, _, window, cx| on(p, window, cx)))
        .into_any_element()
}

impl Render for Picker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(d) = &self.dlg else { return div().into_any_element() };
        let (skip, term, focus) = (d.skip, d.term, d.focus);
        let mut body = div().id("cr-body").flex().flex_col().gap(T::SP_6).overflow_y_scroll().track_scroll(&self.scroll)
            .max_h(T::DIALOG_MAX_H - T::SP_48 * 3.);
        let mut tail: Option<AnyElement> = None;
        match d.heavy.and_then(|i| d.list.get(i).map(|e| (i, e.clone()))) {
            Some((_, e)) => {
                body = body
                    .child(div().text_size(T::TEXT_13).line_height(relative(T::LH_CHAT)).text_color(T::text_secondary())
                        .child(div().font_weight(T::W_BOLD).text_color(T::text()).child(e.title.clone()))
                        .child(resume::heavy_intro(e.tokens)))
                    .child(row("cr-compact".into(), focus == 0, true, "Compact first (recommended)".into(), None, |p, w, cx| p.heavy("compact", w, cx), cx))
                    .child(row("cr-full".into(), focus == 1, false, "Load the full conversation".into(), None, |p, w, cx| p.heavy("resume", w, cx), cx));
                tail = Some(div().id("cr-back").cursor_pointer().font_family(T::MONO).text_size(T::TEXT_12)
                    .text_color(if focus == 2 { T::accent_hover() } else { T::text_muted() }).hover(|d| d.text_color(T::text()))
                    .child("← Back to the list").on_click(cx.listener(|p, _, _, cx| p.back(cx))).into_any_element());
            }
            None => {
                let rows: Vec<(usize, Earlier)> = d.list.iter().cloned().enumerate().collect();
                let error = d.error.clone();
                body = body.child(row("cr-new".into(), focus == 0, true, "New session".into(),
                    Some(div().text_size(T::TEXT_11).font_family(T::MONO).text_color(T::text_muted()).child("Start a fresh conversation").into_any_element()),
                    |p, w, cx| p.new_session(w, cx), cx));
                if !error.is_empty() {
                    body = body.child(div().px(T::SP_2).py(T::SP_10).text_size(T::TEXT_13).text_color(T::text_muted()).child(format!("Could not load earlier conversations: {error}")));
                }
                for (i, e) in rows {
                    let meta = div().flex().flex_wrap().gap(T::SP_6).text_size(T::TEXT_11).font_family(T::MONO).text_color(T::text_muted())
                        .child(e.meta.clone())
                        .when(e.heavy, |d| d.child(div().font_weight(T::W_BOLD).text_color(T::yellow()).child("heavy")))
                        .when(e.in_use, |d| d.child(div().font_weight(T::W_BOLD).text_color(T::accent()).child("open elsewhere — opens a copy")));
                    body = body.child(row(SharedString::from(format!("cr-{i}")), focus == i + 1, false, e.title.clone(), Some(meta.into_any_element()), move |p, w, cx| p.pick(i, w, cx), cx));
                }
            }
        }
        let last = self.last_focus();
        div().track_focus(&self.focus).key_context("ResumePicker").capture_key_down(cx.listener(|p, ev, w, cx| p.key(ev, w, cx)))
            .on_action(cx.listener(|p, _: &PickerTab, _, cx| p.step(1, true, cx)))
            .on_action(cx.listener(|p, _: &PickerBacktab, _, cx| p.step(-1, true, cx)))
            .flex().flex_col().w_full()
            .child(div().text_size(T::TEXT_16).font_weight(T::W_BOLD).text_color(T::text()).pb(T::SP_12).child(heading(skip, term)))
            .child(body)
            .children(tail.map(|t| div().pt(T::SP_8).child(t)))
            .child(div().flex().justify_end().gap(T::SP_8).pt(T::SP_16)
                .child(div().rounded(T::R_CONTROL).when(focus == last && d.heavy.is_none(), |x| x.border_1().border_color(T::accent_hover()))
                    .child(w::button("cr-cancel", "Cancel").on_click(cx.listener(|p, _, w, cx| p.cancel(w, cx))))))
            .into_any_element()
    }
}

/// Driver actions for no chat in particular: the picker's buttons, and
/// `resume-open [skip] [term]` (the picker alone, as a launcher would open it).
pub fn global_act(name: &str, args: &[String], window: &mut Window, cx: &mut App) -> Option<Value> {
    let p = picker(cx);
    let a = |i: usize| args.get(i).cloned().unwrap_or_default();
    match name {
        "resume-open" => {
            let project = crundi_shell::project(cx);
            let ok = choose(&project, a(0) == "skip" || a(0) == "true", a(1) == "term" || a(1) == "true", cx, |_, _, _| {});
            return Some(json!({ "ok": ok }));
        }
        "resume-new" => p.update(cx, |p, cx| p.new_session(window, cx)),
        "resume-pick" => { let i = a(0).parse().unwrap_or(0); p.update(cx, |p, cx| p.pick(i, window, cx)) }
        "resume-compact" => p.update(cx, |p, cx| p.heavy("compact", window, cx)),
        "resume-full" => p.update(cx, |p, cx| p.heavy("resume", window, cx)),
        "resume-back" => p.update(cx, |p, cx| p.back(cx)),
        "resume-cancel" => p.update(cx, |p, cx| p.cancel(window, cx)),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// The picker's props (Null while it is closed), and the last answer.
pub fn global_props(cx: &App) -> (Value, Value) {
    match cx.try_global::<PickerHandle>() {
        Some(h) => { let p = h.0.read(cx); (p.props(), p.last.as_ref().map(|c| json!({ "mode": c.mode, "resumeId": c.resume_id })).unwrap_or(Value::Null)) }
        None => (Value::Null, Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn the_fetch_outcome() {
        let v = json!({ "ok": true, "sessions": [{ "id": "aaaa1111-x", "title": "Docs", "updatedAt": "2026-01-01T00:00:00Z", "tokens": 38100 }] });
        let (l, e) = outcome(Ok(v), 0.0);
        assert_eq!((l.len(), e.as_str()), (1, ""));
        // An old server (404): an empty list, no error: a new session at once.
        let (l, e) = outcome(Err(ApiError::Status(404, "Not found".into()).to_string()), 0.0);
        assert!(l.is_empty() && e.is_empty());
        let (l, e) = outcome(Err(ApiError::Net("Failed to fetch".into()).to_string()), 0.0);
        assert!(l.is_empty());
        assert_eq!(e, "Failed to fetch");
    }

    #[::core::prelude::v1::test]
    fn headings() {
        assert_eq!(heading(false, false), "Open UI Mode");
        assert_eq!(heading(true, false), "Open UI Mode — Skip Permissions");
        assert_eq!(heading(true, true), "Open Terminal — Skip Permissions");
    }

    #[::core::prelude::v1::test]
    fn stubs() {
        assert!(matches!(stub_result(json!({ "status": 404, "error": "Not found" })), Err(ApiError::Status(404, _))));
        assert!(matches!(stub_result(json!({ "net": "Failed to fetch" })), Err(ApiError::Net(_))));
        assert!(stub_result(json!({ "ok": true, "sessions": [] })).is_ok());
    }
}
