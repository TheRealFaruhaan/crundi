//! 2.6: the permission mode (the web's .cc-mode button and its cc-modal,
//! openModeModal): the meta row's button (the mode's icon and name, its
//! colour; the icon alone when narrow), the dialog over the window listing
//! the chat's modes (Bypass all first for a chat launched with skip
//! permissions), each with its icon, name and description, a tick on the
//! one in effect. A press on the scrim, ×, or Esc closes it (as the web's);
//! Tab / Shift+Tab move between × and the modes, ↑ ↓ between the modes,
//! Enter or Space chooses. A choice shows at once and goes to the server
//! (POST /permission-mode); refused or failed, the button goes back and a
//! toast says why. The mode set elsewhere arrives in the chat's meta.

use super::Composer;
use crate::store::{ChatMeta, ChatStore};
use crundi_core::head;
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::*;
use serde_json::{json, Value};

/// The web's MODE_INFO icons.
pub fn icon(m: &str) -> IconName {
    match m {
        "bypassPermissions" => IconName::ShieldAlert,
        "acceptEdits" => IconName::PenLine,
        "auto" => IconName::Zap,
        "plan" => IconName::ClipboardList,
        "dontAsk" => IconName::Ban,
        _ => IconName::ShieldQuestionMark,
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

/// The chat's mode as shown (its own when in its list, else Ask permissions).
pub fn current(c: &Composer) -> &'static str { head::shown_mode(&c.meta.mode, c.meta.skip) }

/// The meta row's button.
pub fn button(c: &Composer, narrow: bool, fs: f32, cx: &mut Context<Composer>) -> AnyElement {
    let key = current(c);
    let (name, _) = head::mode_info(key);
    let (fg, edge, keep) = tone(key);
    w::tone_button("mode", icon(key), (!narrow).then(|| SharedString::from(name)), fg, edge, keep, format!("Permission mode: {name}. Click to change."), fs)
        .on_click(cx.listener(|this, _, window, cx| open(this, window, cx))).into_any_element()
}

/// The dialog's body (a view in the modal layer).
pub struct ModePicker {
    store: Entity<ChatStore>,
    /// The keyboard's focus: -1 the ×, else a mode's index.
    focus: i32,
    start: i32,
    handle: FocusHandle,
    _sub: Subscription,
}

struct Open(WeakEntity<ModePicker>);
impl Global for Open {}

fn modes_of(store: &Entity<ChatStore>, cx: &App) -> (Vec<&'static str>, &'static str) {
    let m = &store.read(cx).meta.read(cx).0;
    (head::modes(m.skip), head::shown_mode(&m.mode, m.skip))
}

/// Open the dialog on the chat's mode (the keyboard's focus on it).
pub fn open(c: &mut Composer, window: &mut Window, cx: &mut Context<Composer>) {
    let store = c.store.clone();
    let (list, cur) = modes_of(&store, cx);
    let i = list.iter().position(|m| *m == cur).unwrap_or(0) as i32;
    let meta = store.read(cx).meta.clone();
    let picker = cx.new(|cx| ModePicker { store, focus: i, start: i, handle: cx.focus_handle(), _sub: cx.observe(&meta, |_, _: Entity<ChatMeta>, cx| cx.notify()) });
    let h = picker.read(cx).handle.clone();
    cx.set_global(Open(picker.downgrade()));
    w::modal(window, cx, T::DIALOG_W, picker.into(), |_, cx| { cx.remove_global::<Open>(); });
    window.focus(&h, cx);
}

fn picker(cx: &App) -> Option<Entity<ModePicker>> { cx.try_global::<Open>().and_then(|o| o.0.upgrade()).filter(|_| w::modal_open(cx)) }

fn close(window: &mut Window, cx: &mut App) { if picker(cx).is_some() { w::close_dialog(window, cx); } }

/// A mode chosen: closed; another mode shows at once and goes to the server.
fn choose(store: Entity<ChatStore>, key: &str, window: &mut Window, cx: &mut App) {
    close(window, cx);
    let (list, prev) = modes_of(&store, cx);
    let Some(key) = list.into_iter().find(|m| *m == key) else { return };
    if key == prev { return; }
    store.update(cx, |s, cx| s.edit_conv(|c| c.mode = key.to_string(), cx));
    let id = store.read(cx).id.clone();
    let weak = store.downgrade();
    crundi_shell::call(cx, move |api| api.post(&format!("/api/ui-sessions/{id}/permission-mode"), &json!({ "mode": key })), move |r, cx| {
        let err = match r {
            Ok(v) if v.get("ok").and_then(|o| o.as_bool()) == Some(false) => Some(v.get("error").and_then(|e| e.as_str()).unwrap_or("Refused").to_string()),
            Ok(_) => None,
            Err(e) => Some(e),
        };
        let Some(e) = err else { return };
        // Back to the mode in effect (unless it moved on meanwhile).
        let _ = weak.update(cx, |s, cx| if s.conv.mode == key { s.edit_conv(|c| c.mode = prev.to_string(), cx); });
        super::toast(cx, &head::mode_error(&e), true);
    });
}

impl ModePicker {
    fn key(&mut self, key: &str, shift: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let (list, _) = modes_of(&self.store, cx);
        let n = list.len() as i32;
        match key {
            "escape" => close(window, cx),
            "tab" => {
                // × is -1, the modes 0..n: round and round, never out of the dialog.
                let by = if shift { -1 } else { 1 };
                self.focus = (self.focus + 1 + by).rem_euclid(n + 1) - 1;
                cx.notify();
            }
            "enter" | "space" => {
                if self.focus < 0 { close(window, cx); } else if let Some(m) = list.get(self.focus as usize) { choose(self.store.clone(), m, window, cx); }
            }
            "up" | "down" => {
                self.focus = (self.focus + if key == "up" { -1 } else { 1 }).clamp(0, n - 1);
                cx.notify();
            }
            _ => return false,
        }
        true
    }
}

impl Render for ModePicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (list, cur) = modes_of(&self.store, cx);
        let mut col = div().id("mode-picker").track_focus(&self.handle).flex().flex_col().w_full()
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| if this.key(&e.keystroke.key, e.keystroke.modifiers.shift, window, cx) { cx.stop_propagation(); }))
            .child(w::modal_head("mode-x", "Permission mode", self.focus == -1, |_, window, cx| close(window, cx)));
        for (i, m) in list.into_iter().enumerate() {
            let (name, desc) = head::mode_info(m);
            let store = self.store.clone();
            col = col.child(w::choice_row(("mode-opt", i), icon(m), tone(m).0, name, desc, m == cur, self.focus == i as i32)
                .on_click(move |_, window, cx| choose(store.clone(), m, window, cx)));
        }
        col
    }
}

// ─── The test driver ───

pub fn test_ui(c: &mut Composer, name: &str, args: &[String], window: &mut Window, cx: &mut Context<Composer>) -> Option<Value> {
    match name {
        "mode-open" => open(c, window, cx),
        "mode-choose" => { let k = args.first().cloned().unwrap_or_default(); choose(c.store.clone(), &k, window, cx) }
        "mode-close" => close(window, cx),
        "mode-key" => {
            // "tab", "shift-tab", "down", "enter", "escape"…
            let src = args.first().cloned().unwrap_or_default();
            let (shift, key) = match src.strip_prefix("shift-") { Some(k) => (true, k.to_string()), None => (false, src.clone()) };
            if let Some(p) = picker(cx) { p.update(cx, |p, cx| p.key(&key, shift, window, cx)); }
        }
        _ => return None,
    }
    Some(json!({ "ok": true }))
}

/// mode (its name), modeKey, planning (the header's PLANNING tag), and the
/// dialog while open: its modes, the one in effect, the keyboard's focus.
pub fn state(c: &Composer, cx: &App) -> Value {
    let key = current(c);
    let dialog = picker(cx).map(|p| {
        let p = p.read(cx);
        let (list, cur) = modes_of(&p.store, cx);
        json!({ "modes": list.iter().map(|m| { let (n, d) = head::mode_info(m); json!({ "key": m, "name": n, "desc": d }) }).collect::<Vec<_>>(),
            "current": cur, "start": p.start, "focus": p.focus, "chat": p.store.read(cx).id })
    });
    json!({ "mode": head::mode_info(key).0, "modeKey": key, "planning": key == "plan" && !c.meta.has_plan, "modePicker": dialog })
}
