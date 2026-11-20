//! The PC lock (capability 'pc-lock'; src/webapp-html.js applyPcLock, the
//! #pc-lock screen; app/main.js pcLockAsk, the auto lock, wirePcLockKeys):
//! this app's window locked behind the owner's PIN (the kid mode PIN).
//! "visible": the app stays in view behind a see-through overlay with the
//! PIN box at the bottom; "full": an opaque lock screen with the time, the
//! date, the PIN box and its keypad. "Hey Cru" works as always: Cru's screen
//! is an overlay above this one.
//!
//! The state is the server's: GET /api/lock at sign-in, the `pc-lock` event
//! (SSE) and the desktop socket's { type: "pc-lock", lock }. The tray's
//! "Lock (visible)" / "Lock (full)", Ctrl+Shift+L and Super+Shift+L ask
//! POST /api/lock/start; the PIN goes to POST /api/lock/unlock. While it is
//! locked the window is locked as kid mode's (`ctx::lock_window`).
//!
//! Auto lock ({ on, minutes, mode } from the server): after that long with
//! no keyboard or mouse anywhere on the PC (or when the OS's screen locks),
//! once per idle stretch. The idle time is read on a thread of its own every
//! 15 s, and only while auto lock is on (`idle.rs`); the UI hears of it only
//! when it fires.

use crate::{ctx, cru_kid};
use crundi_theme::{IconName, T};
use crundi_widgets as w;
use gpui::{prelude::FluentBuilder as _, *};
use serde_json::{json, Value};
use std::{cell::RefCell, time::Duration};

#[derive(Clone, Default, PartialEq, Debug)]
pub struct Lock {
    pub on: bool,
    /// "visible" or "full".
    pub mode: String,
    pub err: String,
    /// Bumped by a wrong PIN (the box shakes).
    pub shakes: u32,
    pub auto: Auto,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Auto { pub on: bool, pub minutes: u64, pub mode: String }
impl Default for Auto { fn default() -> Self { Auto { on: false, minutes: 10, mode: "full".into() } } }

thread_local! { static L: RefCell<Lock> = RefCell::new(Lock::default()); }
pub fn with<R>(f: impl FnOnce(&mut Lock) -> R) -> R { L.with(|l| f(&mut l.borrow_mut())) }
pub fn on() -> bool { with(|l| l.on) }

/// The error the web shows when the server says no without a reason.
pub const NEEDS_PIN: &str = "The PC lock needs your PIN: set it in Settings, Voice assistant";

/// At sign-in: the lock as the server keeps it (a restarted app locks again).
pub fn load() {
    ctx::call(|api| api.get("/api/lock"), |r| if let Ok(st) = r {
        if !ctx::truthy(&st["ok"]) { return; }
        if ctx::truthy(&st["on"]) { apply(&st); } else { set_auto(&st["auto"]); }
    });
}

/// The server's lock state (its `pc-lock` event, an answer).
pub fn apply(st: &Value) {
    set_auto(&st["auto"]);
    let on = ctx::truthy(st.get("on").unwrap_or(&Value::Null));
    let was = on_now();
    if !on {
        with(|l| { l.on = false; l.err.clear(); });
        if was && !cru_kid::on() { ctx::lock_window(false); }
        ctx::dirty();
        return;
    }
    let mode = if ctx::s(st, "mode") == "visible" { "visible" } else { "full" };
    with(|l| { l.on = true; l.mode = mode.into(); });
    ctx::lock_window(true);
    ctx::dirty();
}
fn on_now() -> bool { on() }

fn set_auto(a: &Value) {
    if !a.is_object() { return; }
    let auto = Auto {
        on: a["on"].as_bool().unwrap_or(false),
        minutes: a["minutes"].as_u64().unwrap_or(10).max(1),
        mode: if ctx::s(a, "mode") == "visible" { "visible".into() } else { "full".into() },
    };
    with(|l| l.auto = auto.clone());
    crate::idle::set(auto.on, Duration::from_secs(auto.minutes * 60));
}

/// The tray's items, the shortcuts, the auto lock: ask the server to lock
/// ("visible" or "full"; `by` "shortcut", "tray" or "auto"). Not while kid
/// mode holds the window, nor when it is already locked.
pub fn ask(mode: &str, by: &str) {
    if cru_kid::on() { return; }
    ctx::raise();
    if on() { return; }
    let body = json!({ "mode": if mode == "visible" { "visible" } else { "full" }, "by": if by == "auto" { "auto" } else { "shortcut" } });
    ctx::call(move |api| api.post("/api/lock/start", &body), |r| match r {
        Ok(d) if ctx::truthy(&d["ok"]) => apply(&d),
        Ok(d) => ctx::toast_error(&Some(ctx::s(&d, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| NEEDS_PIN.into())),
        // The server's refusal comes as an error with its words.
        Err(e) => ctx::toast_error(&error_words(&e).unwrap_or_else(|| NEEDS_PIN.into())),
    });
}

/// An API error's own words (the body's "error"), when the server gave them.
fn error_words(e: &str) -> Option<String> {
    let generic = e.is_empty() || e.starts_with("The server answered") || e.starts_with("Cannot reach") || e.starts_with("Signed out") || e.starts_with("Not supported");
    (!generic).then(|| e.to_string())
}

/// The PIN typed: unlocked, or the server's reason ("That is not the PIN",
/// "Too many tries…") and a shake.
pub fn try_pin(pin: &str) {
    let pin = pin.trim().to_string();
    if pin.is_empty() { return; }
    let body = json!({ "pin": pin });
    ctx::call(move |api| api.post("/api/lock/unlock", &body), |r| match r {
        Ok(d) if ctx::truthy(&d["ok"]) => apply(&d),
        Ok(d) => wrong(Some(ctx::s(&d, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "That did not work".into())),
        Err(e) => wrong(error_words(&e).unwrap_or_else(|| "That did not work".into())),
    });
}

fn wrong(err: String) {
    with(|l| { l.err = err; l.shakes += 1; });
    ctx::dirty();
}

pub fn clear_error() { if with(|l| !l.err.is_empty()) { with(|l| l.err.clear()); ctx::dirty(); } }

pub fn props() -> Value {
    with(|l| json!({ "on": l.on, "mode": l.mode, "error": l.err, "shakes": l.shakes, "auto": { "on": l.auto.on, "minutes": l.auto.minutes, "mode": l.auto.mode }, "idleWatch": crate::idle::watching() }))
}

// ─── The lock screen (#pc-lock) ───

/// The keypad's look (.pcl-keys button).
fn pad_look() -> w::PinLook {
    w::PinLook { key_w: T::PCL_KEY_W, key_h: T::PCL_KEY_H, gap: T::SP_10, radius: T::PCL_KEY_R, bg: T::pcl_key(), hover: Some(T::pcl_key_hover()), edge: T::pcl_key_edge(), fg: T::white(), text: T::TEXT_20, icons: true }
}

/// The screen over the window while locked (an overlay under Cru's).
pub struct PcLockScreen {
    copy: Lock,
    input: Option<Entity<w::InputState>>,
    /// The minute the clock shows (it redraws only when that changes).
    minute: i64,
    _subs: Vec<Subscription>,
}

impl PcLockScreen {
    pub fn new(_cx: &mut Context<Self>) -> Self { PcLockScreen { copy: Lock::default(), input: None, minute: 0, _subs: vec![] } }

    /// With the window: the PIN box (Enter tries it, a key clears the error).
    pub fn attach(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.is_some() { return; }
        let input = cx.new(|cx| w::InputState::new(window, cx).masked(true).placeholder("PIN")
            .validate(|s, _| s.chars().count() <= 8));
        self._subs.push(cx.subscribe_in(&input, window, |_, st, ev: &w::InputEvent, window, cx| match ev {
            w::InputEvent::PressEnter { .. } => {
                let v = st.read(cx).value().to_string();
                st.update(cx, |s, cx| s.set_value("", window, cx));
                try_pin(&v);
            }
            w::InputEvent::Change => clear_error(),
            _ => {}
        }));
        self.input = Some(input);
    }

    /// The pump: the copy taken again; notified only when it moved.
    pub fn pump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = with(|l| l.clone());
        if now == self.copy { return; }
        let opened = now.on && !self.copy.on;
        let shook = now.shakes != self.copy.shakes;
        self.copy = now;
        if let Some(i) = self.input.clone() {
            if opened || shook { i.update(cx, |s, cx| s.set_value("", window, cx)); }
            if opened { let f = i.read(cx).focus_handle(cx); window.focus(&f, cx); }
        }
        if opened { self.tick(cx); }
        cx.notify();
    }

    /// The clock: redrawn at the next minute, only while locked in full.
    fn tick(&mut self, cx: &mut Context<Self>) {
        let now = chrono::Local::now();
        self.minute = now.timestamp() / 60;
        let wait = Duration::from_millis((60_000 - (now.timestamp_millis().rem_euclid(60_000))) as u64 + 50);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |s, cx| if s.copy.on && s.copy.mode == "full" { s.tick(cx); cx.notify(); });
        }).detach();
    }

    pub fn press(&self, key: &'static str, window: &mut Window, cx: &mut App) {
        let Some(i) = self.input.clone() else { return };
        let v = i.read(cx).value().to_string();
        match key {
            "OK" => { i.update(cx, |s, cx| s.set_value("", window, cx)); try_pin(&v); }
            "Back" => { let mut v = v; v.pop(); i.update(cx, |s, cx| s.set_value(v, window, cx)); clear_error(); }
            d => { if v.chars().count() < 8 { i.update(cx, |s, cx| s.set_value(format!("{v}{d}"), window, cx)); } clear_error(); }
        }
        let f = i.read(cx).focus_handle(cx);
        window.focus(&f, cx);
    }
}

impl Render for PcLockScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let l = &self.copy;
        // The clock's size: clamp(48px, min(11vw, 14vh), 140px).
        let vp = window.viewport_size();
        let time_px = px((f32::from(vp.width) * 0.11).min(f32::from(vp.height) * 0.14).clamp(48., f32::from(T::PCL_TIME)));
        let date_px = px((f32::from(vp.width) * 0.022).clamp(18., f32::from(T::PCL_DATE)));
        if !l.on { return div().id("pc-lock-off").into_any_element(); }
        let full = l.mode != "visible";
        let now = chrono::Local::now();
        let input = self.input.clone();
        let this = cx.entity().downgrade();
        let badge = div().absolute().top(T::PCL_TOP).left_0().right_0().flex().justify_center()
            .child(div().flex().items_center().gap(T::SP_8).px(T::SP_16).py(T::SP_8).rounded(T::R_PILL)
                .bg(T::pcl_badge()).border_1().border_color(T::pcl_badge_edge()).text_size(T::TEXT_14).font_weight(T::W_SEMIBOLD)
                .child(w::icon(IconName::Lock, T::ICON_MD)).child("Locked"));
        let pin = input.as_ref().map(|i| w::input(i).w(if full { T::PCL_IN_W } else { T::PCL_IN_W_VIS }).text_center()
            .text_size(if full { T::TEXT_24 } else { T::TEXT_18 }).font_family(T::MONO));
        let err = div().min_h(T::PCL_ERR_H).text_size(T::TEXT_13).text_color(T::pcl_err()).child(l.err.clone());
        let mut boxed = div().id("pc-lock-box").flex().items_center().rounded(T::PCL_BOX_R).bg(T::pcl_box()).border_1().border_color(T::pcl_box_edge()).shadow_lg()
            .when(full, |d| d.flex_col().gap(T::SP_10).px(T::SP_22).py(T::SP_20))
            .when(!full, |d| d.flex_row().gap(T::SP_10).px(T::SP_14).py(T::SP_10))
            .when_some(pin, |d, p| d.child(p));
        if full {
            let pad = w::pin_pad("pcl-key", pad_look(), move |k, window, cx| { let _ = this.update(cx, |s, cx| { s.press(k, window, cx); }); });
            boxed = boxed.child(pad);
        }
        boxed = boxed.child(err).when(full, |d| d.child(div().text_size(T::TEXT_12).opacity(T::PCL_HINT).child("Say Hey Cru to talk to Cru")));
        // A wrong PIN: the box shakes (.4 s).
        let boxed = boxed.with_animation(("pcl-shake", l.shakes as usize), Animation::new(Duration::from_millis(if l.shakes == 0 { 1 } else { 400 })), move |d, t| {
            let x = if t >= 1. { 0. } else { (t * std::f32::consts::PI * 4.).sin() * 8. };
            d.relative().left(px(x))
        });
        let focus_in = input.clone();
        let mut root = div().id("pc-lock").absolute().inset_0().occlude().flex().flex_col().items_center().text_color(T::white())
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                // A click anywhere goes nowhere but back to the PIN box.
                cx.stop_propagation();
                if let Some(i) = focus_in.as_ref() { let f = i.read(cx).focus_handle(cx); window.focus(&f, cx); }
            })
            .on_key_down(|_, _, cx| cx.stop_propagation());
        if full {
            root = root.justify_between().pt(relative(0.07)).pb(relative(0.05))
                .child(canvas(|_, _, _| (), |b, _, window, _| {
                    window.paint_quad(fill(b, T::pcl_full()));
                    let w = f32::from(b.size.width);
                    let h = f32::from(b.size.height);
                    w::radial_glow(window, point(b.origin.x + px(w * 0.3), b.origin.y + px(h * 0.2)), 720., T::pcl_glow_a(), 0.85);
                    w::radial_glow(window, point(b.origin.x + px(w * 0.8), b.origin.y + px(h * 0.9)), 540., T::pcl_glow_b(), 0.85);
                }).absolute().inset_0())
                .child(div())
                .child(div().relative().flex().flex_col().items_center()
                    .child(div().text_size(time_px).font_weight(FontWeight::LIGHT).line_height(relative(1.05)).child(now.format("%H:%M").to_string()))
                    .child(div().text_size(date_px).opacity(0.9).child(now.format("%A %-d %B").to_string())))
                .child(boxed);
        } else {
            root = root.justify_end().pb(relative(0.05)).bg(T::pcl_vis()).border(T::SP_4).border_color(T::pcl_vis_ring()).child(boxed);
        }
        root.child(badge).into_any_element()
    }
}
