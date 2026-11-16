//! 2.6: the permission-mode picker (the web's cc-modal, specs/mode-picker.md):
//! the meta row's mode button (its icon, name and colour per mode), the
//! dialog over the window listing the chat's modes (Bypass all first for a
//! chat launched with skip permissions), the keyboard (Tab / Shift+Tab
//! between × and the modes, Enter or Space chooses, Esc closes), and the
//! change: shown at once, POST /permission-mode; refused or failed, the
//! button goes back and a toast says why (a deliberate difference, PLAN.md).
//! The mode set elsewhere (another client, the CLI) arrives through the
//! chat model (init / meta events).

use crate::ComposerView;
use crundi_core::head;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Value};
use std::rc::Rc;

/// The picker of one chat's composer.
#[derive(Default)]
pub struct State {
    pub open: bool,
    /// The keyboard's focus: -1 the ×, else a mode's index.
    pub focus: i32,
    /// The mode's index selected when it opened.
    pub start: i32,
    pub focus_handle: Option<FocusHandle>,
    pub scroll: ScrollHandle,
    /// Frames left in which the focus is scrolled into view (the first
    /// layout after opening may still hold the last open's bounds).
    pub follow: u8,
}

/// The chat's mode as shown (its own when in its list, else Ask permissions).
pub fn current(c: &ComposerView, cx: &App) -> &'static str {
    let m = c.model.read(cx);
    head::shown_mode(&m.conv.mode, m.conv.skip)
}

fn modes(c: &ComposerView, cx: &App) -> Vec<&'static str> { head::modes(c.model.read(cx).conv.skip) }

fn icon(m: &str) -> IconName {
    match m {
        "bypassPermissions" => IconName::ShieldOff,
        "acceptEdits" => IconName::FilePen,
        "auto" => IconName::Sparkles,
        "plan" => IconName::ClipboardList,
        "dontAsk" => IconName::Ban,
        _ => IconName::Shield,
    }
}

/// A mode's colour (icon and name), its border, and whether hovering keeps it.
fn tone(m: &str) -> (Hsla, Hsla, bool) {
    match m {
        "bypassPermissions" => (T::red(), T::error_edge(), true),
        "plan" => (T::sky(), T::composer_sky_edge(), true),
        "acceptEdits" | "auto" => (T::accent_hover(), T::border(), false),
        "dontAsk" => (T::yellow(), T::border(), false),
        _ => (T::text_secondary(), T::border(), false),
    }
}

/// The meta row's mode button: icon and name (wide), the icon alone (narrow).
pub fn button(c: &ComposerView, narrow: bool, fs: f32, cx: &mut Context<ComposerView>) -> AnyElement {
    let key = current(c, cx);
    let (name, _) = head::mode_info(key);
    let (fg, edge, keep) = tone(key);
    let b = w::tone_button("mode", icon(key), (!narrow).then(|| SharedString::from(name)), fg, edge, keep, format!("Permission mode: {name}. Click to change."), fs);
    let b = if narrow { b.min_w(T::CONTROL * fs).h(T::CONTROL_SM * fs) } else { b };
    b.on_click(cx.listener(|this, _, window, cx| open(this, window, cx))).into_any_element()
}

/// Open the picker on the chat's mode (the keyboard's focus on it).
pub fn open(c: &mut ComposerView, window: &mut Window, cx: &mut Context<ComposerView>) {
    let list = modes(c, cx);
    let cur = current(c, cx);
    let i = list.iter().position(|m| *m == cur).unwrap_or(0) as i32;
    let h = c.mode.focus_handle.get_or_insert_with(|| cx.focus_handle()).clone();
    c.mode.open = true;
    c.mode.focus = i;
    c.mode.start = i;
    { c.mode.scroll.scroll_to_item(i as usize + 1); c.mode.follow = 3; }
    window.focus(&h, cx);
    cx.notify();
}

pub fn close(c: &mut ComposerView, cx: &mut Context<ComposerView>) {
    if !c.mode.open { return; }
    c.mode.open = false;
    c.mode.scroll.set_offset(point(px(0.), px(0.)));
    cx.notify();
}

/// A mode chosen: closed; another mode shows at once and goes to the server.
pub fn choose(c: &mut ComposerView, key: &str, cx: &mut Context<ComposerView>) {
    close(c, cx);
    let prev = current(c, cx).to_string();
    let Some(key) = modes(c, cx).into_iter().find(|m| *m == key) else { return };
    if key == prev { return; }
    c.model.update(cx, |m, cx| { m.conv.mode = key.to_string(); cx.notify(); });
    let id = c.chat_id(cx);
    let model = c.model.downgrade();
    crundi_shell::call(cx, move |api| api.post(&format!("/api/ui-sessions/{id}/permission-mode"), &json!({ "mode": key })), move |r, cx| {
        let err = match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Some(v.get("error").and_then(|e| e.as_str()).unwrap_or("Refused").to_string()),
            Ok(_) => None,
            Err(e) => Some(e),
        };
        let Some(e) = err else { return };
        // Back to the mode in effect (unless it moved on meanwhile).
        let _ = model.update(cx, |m, cx| if m.conv.mode == key { m.conv.mode = prev.clone(); cx.notify(); });
        crate::attach::toast(cx, &head::mode_error(&e), true);
    });
}

/// The dialog's keys (Esc, Tab, Shift+Tab, Enter, Space).
fn key(c: &mut ComposerView, ks: &Keystroke, cx: &mut Context<ComposerView>) -> bool {
    let n = modes(c, cx).len() as i32;
    let k = ks.key.as_str();
    match k {
        "escape" => close(c, cx),
        "tab" => {
            // × is -1; the modes 0..n; round and round, never out of the dialog.
            let by = if ks.modifiers.shift { -1 } else { 1 };
            let next = (c.mode.focus + 1 + by).rem_euclid(n + 1) - 1;
            c.mode.focus = next;
            { c.mode.scroll.scroll_to_item((next + 1) as usize); c.mode.follow = 3; }
            cx.notify();
        }
        "enter" | "space" => {
            if c.mode.focus < 0 { close(c, cx); } else if let Some(m) = modes(c, cx).get(c.mode.focus as usize) { choose(c, m, cx); }
        }
        "up" | "down" => {
            let by = if k == "up" { -1 } else { 1 };
            c.mode.focus = (c.mode.focus + by).clamp(0, n - 1);
            { c.mode.scroll.scroll_to_item((c.mode.focus + 1) as usize); c.mode.follow = 3; }
            cx.notify();
        }
        _ => return false,
    }
    true
}

/// The picker's keys come first (before any binding: the window's own Tab
/// handling would otherwise take Tab out of the dialog): an interceptor.
pub fn init(cx: &mut App) {
    cx.intercept_keystrokes(|e, _window, cx| {
        let open: Option<Entity<ComposerView>> = cx.try_global::<crate::Composers>()
            .and_then(|c| c.0.values().filter_map(|w| w.upgrade()).find(|e| e.read(cx).mode.open));
        let Some(c) = open else { return };
        let ks = e.keystroke.clone();
        if c.update(cx, |c, cx| key(c, &ks, cx)) { cx.stop_propagation(); }
    }).detach();
}

/// The picker, over the whole window (drawn from the composer, deferred).
pub fn render(c: &mut ComposerView, window: &mut Window, cx: &mut Context<ComposerView>) -> Option<AnyElement> {
    if !c.mode.open { return None; }
    let Some(focus) = c.mode.focus_handle.clone() else { return None };
    if c.mode.follow > 0 {
        c.mode.follow -= 1;
        c.mode.scroll.scroll_to_item((c.mode.focus + 1) as usize);
        cx.notify();
    }
    let me = cx.entity().downgrade();
    let cur = current(c, cx);
    let (me1, me2) = (me.clone(), me.clone());
    let mut children: Vec<AnyElement> = vec![w::modal_head("mode-x", "Permission mode", c.mode.focus == -1, cx.listener(|this, _, _, cx| close(this, cx))).into_any_element()];
    for (i, m) in modes(c, cx).into_iter().enumerate() {
        let (name, desc) = head::mode_info(m);
        children.push(w::choice_row(("mode-opt", i), icon(m), tone(m).0, name, desc, m == cur, c.mode.focus == i as i32)
            .on_click(cx.listener(move |this, _, _, cx| choose(this, m, cx))).into_any_element());
    }
    Some(w::overlay_modal(w::Modal {
        id: "mode-picker",
        width: T::DIALOG_W,
        focus,
        scroll: c.mode.scroll.clone(),
        on_close: Rc::new(move |_, cx| { let _ = me1.update(cx, |this, cx| close(this, cx)); }),
        on_key: Rc::new(move |e, _, cx| { if me2.update(cx, |this, cx| key(this, &e.keystroke, cx)).unwrap_or(false) { cx.stop_propagation(); } }),
    }, window, children).into_any_element())
}

/// mode-open, mode-choose <key>, mode-close.
pub fn test_ui(c: &mut ComposerView, name: &str, args: &[String], window: &mut Window, cx: &mut Context<ComposerView>) -> Option<Value> {
    match name {
        "mode-open" => open(c, window, cx),
        "mode-choose" => { let k = args.first().cloned().unwrap_or_default(); choose(c, &k, cx) }
        "mode-close" => close(c, cx),
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// mode (its name), modeKey, planning (the header's PLANNING tag).
pub fn pane_props(c: &ComposerView, cx: &App) -> Value {
    let key = current(c, cx);
    let planning = key == "plan" && c.model.read(cx).conv.plan.is_none();
    json!({ "mode": head::mode_info(key).0, "modeKey": key, "planning": planning })
}

/// modePicker: null when closed; else the modes, the current one, the
/// keyboard's focus and where it sits in the (scrolling) dialog.
pub fn props(c: &ComposerView, cx: &App) -> Value {
    if !c.mode.open { return json!({ "modePicker": null, "pointerModal": false }); }
    let s = &c.mode.scroll;
    let view = s.bounds();
    let scroll = -f32::from(s.offset().y);
    let (top, bottom) = s.bounds_for_item((c.mode.focus + 1) as usize)
        .map(|b| (f32::from(b.top() - view.top()), f32::from(b.bottom() - view.top()))).unwrap_or((0., 0.));
    json!({ "pointerModal": true, "modePicker": {
        "modes": modes(c, cx).iter().map(|m| { let (n, d) = head::mode_info(m); json!({ "key": m, "name": n, "desc": d }) }).collect::<Vec<_>>(),
        "current": current(c, cx), "start": c.mode.start,
        // (null while the dialog is still scrolling the focus into view: GPUI
        // scrolls a child into view at the next layouts, not at once)
        "focus": if c.mode.follow > 0 { Value::Null } else { json!(c.mode.focus) },
        "focusTop": top, "focusBottom": bottom, "scroll": scroll, "viewH": f32::from(view.size.height),
    } })
}
