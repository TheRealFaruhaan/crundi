//! The test driver's commands, answered on the UI thread (the protocol is
//! crundi_core::driver's): the window's own actions here, every area's
//! through the registry (unprefixed names are offered to each area in turn,
//! "<area>:<name>" to that area). It reads through the stores' handles and
//! acts through the same methods the views call; it **never** redraws the
//! window (ARCHITECTURE.md, "The test driver"): what an action changes
//! notifies the store it changed, as a click would.

use crundi_core::driver::Request;
use crundi_shell::{registry, Stores};
use gpui::*;
use serde_json::{json, Map, Value};
use std::sync::atomic::Ordering;

pub fn handle(r: Request, stores: &Stores, window: &mut Window, cx: &mut App) {
    let answer = match r.name() {
        "state" | "props" => json!({ "ok": true, "state": Value::Object(state(stores, window, cx)) }),
        "snapshot" => snapshot(window, &r.str("path")),
        "resize" => { window.resize(size(px(r.num("w") as f32), px(r.num("h") as f32))); json!({ "ok": true }) }
        "mouse" => { mouse(&r, window, cx); json!({ "ok": true }) }
        "key" => { key(&r, window, cx); json!({ "ok": true }) }
        "ui" => ui(&r, stores, window, cx),
        other => json!({ "ok": false, "error": format!("unknown cmd {other}") }),
    };
    r.answer(answer);
}

/// The window's state: the session, the live link's counters, the
/// projects, the tab, and every area's own.
fn state(stores: &Stores, window: &mut Window, cx: &mut App) -> Map<String, Value> {
    let mut m = Map::new();
    let s = stores.session.read(cx);
    let live = s.stats().map(|st| json!({ "connects": st.connects.load(Ordering::Relaxed), "silentDrops": st.silent_drops.load(Ordering::Relaxed), "streams": st.streams.load(Ordering::Relaxed) })).unwrap_or(Value::Null);
    m.insert("signedIn".into(), json!(s.signed_in()));
    m.insert("online".into(), json!(s.online));
    m.insert("signinError".into(), json!(s.signin_error));
    m.insert("signinStep".into(), json!(format!("{:?}", s.step)));
    m.insert("live".into(), live);
    let p = stores.projects.read(cx);
    m.insert("projects".into(), json!(p.list.iter().map(|x| x.alias.clone()).collect::<Vec<_>>()));
    m.insert("project".into(), json!(p.open));
    m.insert("tab".into(), json!(stores.nav.read(cx).tab));
    m.insert("dialog".into(), crundi_widgets::ask_props(cx));
    m.insert("toast".into(), json!(crundi_widgets::toast_text(cx).to_string()));
    m.insert("renders".into(), crundi_core::driver::renders());
    for a in registry::list(cx) {
        if let Some(f) = &a.state { if let Value::Object(o) = f(stores, window, cx) { m.extend(o); } }
    }
    m
}

fn ui(r: &Request, stores: &Stores, window: &mut Window, cx: &mut App) -> Value {
    let name = r.str("name");
    let a = |i| r.arg(i);
    let session = stores.session.clone();
    match name.as_str() {
        "check-server" => session.update(cx, |s, cx| s.check_server(&a(0), cx)),
        "sign-in-password" => session.update(cx, |s, cx| s.sign_in_password(&a(0), &a(1), &a(2), cx)),
        "sign-in-pair" => session.update(cx, |s, cx| s.sign_in_pair(&a(0), cx)),
        "sign-out" => session.update(cx, |s, cx| s.sign_out("", cx)),
        "select-project" => stores.projects.update(cx, |p, cx| p.pick(&a(0), cx)),
        "tab" => stores.nav.update(cx, |n, cx| n.show_tab(&a(0), cx)),
        "drawer" => stores.nav.update(cx, |n, cx| n.set_drawer(a(0) == "true", cx)),
        "stub" => crundi_core::driver::push_stub(&a(0), serde_json::from_str(&a(1)).unwrap_or(Value::Null)),
        "dialog-answer" => { let v = r.args().get(1).cloned(); if !crundi_widgets::ask_answer(a(0) == "true", v, window, cx) { crundi_widgets::close_dialog(window, cx) } }
        "menu-pick" => { crundi_widgets::menu_pick(&a(0), window, cx); }
        "toasts-clear" => crundi_widgets::clear_toasts(window, cx),
        _ => {
            let (area, short) = match name.split_once(':') { Some((a, n)) => (Some(a.to_string()), n.to_string()), None => (None, name.clone()) };
            for ar in registry::list(cx) {
                if area.as_deref().is_some_and(|x| x != ar.name) { continue; }
                if let Some(f) = &ar.test_ui { if let Some(v) = f(stores, &short, &r.args(), window, cx) { return v; } }
            }
            return json!({ "ok": false, "error": format!("unknown ui action {name}") });
        }
    }
    json!({ "ok": true })
}

fn point_of(r: &Request) -> Point<Pixels> { point(px(r.num("x") as f32), px(r.num("y") as f32)) }

/// The pointer, as a person's: move, click, dblclick, tripleclick, down,
/// up, leave, wheel (dx, dy); drag: down here, moves with the left button
/// held to (toX, toY) in `steps` (default 8), up there (F20). mods: shift,
/// ctrl, alt, meta (a shift-click).
pub fn mouse(r: &Request, window: &mut Window, cx: &mut App) {
    let p = point_of(r);
    let mods: Vec<String> = r.cmd.get("mods").and_then(|m| m.as_array()).map(|m| m.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    let has = |k: &[&str]| mods.iter().any(|m| k.iter().any(|x| m.eq_ignore_ascii_case(x)));
    let m = Modifiers { control: has(&["ctrl", "control"]), shift: has(&["shift"]), alt: has(&["alt"]), platform: has(&["meta", "cmd", "super"]), ..Default::default() };
    let mv = |w: &mut Window, cx: &mut App, b: Option<MouseButton>| { w.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position: p, pressed_button: b, modifiers: m }), cx); };
    let down = |w: &mut Window, cx: &mut App, n: usize| { w.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: p, modifiers: m, click_count: n, first_mouse: false }), cx); };
    let up = |w: &mut Window, cx: &mut App, n: usize| { w.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: p, modifiers: m, click_count: n }), cx); };
    match r.str("kind").as_str() {
        "click" => { mv(window, cx, None); down(window, cx, 1); up(window, cx, 1); }
        "dblclick" => { mv(window, cx, None); down(window, cx, 1); up(window, cx, 1); down(window, cx, 2); up(window, cx, 2); }
        "tripleclick" => { mv(window, cx, None); for n in 1..=3 { down(window, cx, n); up(window, cx, n); } }
        "drag" => {
            mv(window, cx, None);
            down(window, cx, 1);
            let to = point(px(r.num("toX") as f32), px(r.num("toY") as f32));
            let steps = (r.num("steps") as usize).clamp(1, 200);
            let steps = if r.cmd.get("steps").is_some() { steps } else { 8 };
            for i in 1..=steps {
                let f = i as f32 / steps as f32;
                let at = point(p.x + (to.x - p.x) * f, p.y + (to.y - p.y) * f);
                window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position: at, pressed_button: Some(MouseButton::Left), modifiers: m }), cx);
            }
            window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: to, modifiers: m, click_count: 1 }), cx);
        }
        "down" => { mv(window, cx, None); down(window, cx, 1); }
        "up" => { mv(window, cx, Some(MouseButton::Left)); up(window, cx, 1); }
        "leave" => { window.dispatch_event(PlatformInput::MouseExited(MouseExitEvent { position: p, pressed_button: None, modifiers: m }), cx); }
        "wheel" => {
            mv(window, cx, None);
            // dx: sideways (a touchpad's swipe), pixels as a touchpad sends.
            let (dx, dy) = (r.num("dx") as f32, r.num("dy") as f32);
            window.dispatch_event(PlatformInput::Scroll(ScrollEvent { position: p, delta: ScrollDelta::Pixels(point(px(dx), px(dy))), modifiers: m, touch_phase: TouchPhase::Moved, source: ScrollSource::Pointer }), cx);
        }
        // A move with buttons: ["left"] is one with the button held.
        _ => { let held = r.cmd.get("buttons").and_then(|b| b.as_array()).is_some_and(|b| b.iter().any(|x| x.as_str() == Some("left"))); mv(window, cx, held.then_some(MouseButton::Left)) }
    }
}

/// A key press and release to the focused element; a named key (Escape,
/// Return, Up…) or text (each character typed); mods: ctrl, shift, alt, meta.
fn key(r: &Request, window: &mut Window, cx: &mut App) {
    let mods: Vec<String> = r.cmd.get("mods").and_then(|m| m.as_array()).map(|m| m.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    // The Slint client's names ("Control", "Shift", "Alt", "Meta") and short ones.
    let has = |k: &[&str]| mods.iter().any(|m| k.iter().any(|x| m.eq_ignore_ascii_case(x)));
    let modifiers = Modifiers { control: has(&["ctrl", "control"]), shift: has(&["shift"]), alt: has(&["alt"]), platform: has(&["meta", "cmd", "super"]), ..Default::default() };
    let text = r.str("text");
    let named = match text.as_str() {
        "Escape" => Some("escape"), "Return" => Some("enter"), "Backspace" => Some("backspace"), "Tab" => Some("tab"),
        "Backtab" => Some("tab"), "Insert" => Some("insert"), "Up" => Some("up"), "Down" => Some("down"), "Left" => Some("left"),
        "Right" => Some("right"), "Home" => Some("home"), "End" => Some("end"), "Delete" => Some("delete"), "Space" => Some("space"),
        _ => None,
    };
    // dispatch_keystroke, as the platform: the key bindings first, then the
    // focused text input's handler gets the typed character (F20).
    // "phase": "down" presses and holds, "up" lets go (a held shortcut: Cru's push to talk); else both.
    let phase = r.str("phase");
    let press = |key: String, ch: Option<String>, modifiers: Modifiers, window: &mut Window, cx: &mut App| {
        let ks = Keystroke { modifiers, key, key_char: ch };
        if phase != "up" { window.dispatch_keystroke(ks.clone(), cx); }
        if phase != "down" { window.dispatch_event(PlatformInput::KeyUp(KeyUpEvent { keystroke: ks }), cx); }
    };
    match named {
        Some(k) => {
            let m = if text == "Backtab" { Modifiers { shift: true, ..modifiers } } else { modifiers };
            let ch = (k == "space" && !modifiers.control && !modifiers.platform).then(|| " ".to_string());
            press(k.into(), ch, m, window, cx)
        }
        None => for c in text.chars() {
            let k = c.to_lowercase().to_string();
            let typed = (!modifiers.control && !modifiers.platform).then(|| c.to_string());
            press(k, typed, Modifiers { shift: modifiers.shift || c.is_uppercase(), ..modifiers }, window, cx);
        },
    }
}

/// The window as drawn: xwd of its X window, as a PNG (ImageMagick's
/// convert; under Xvfb on the server). GPUI draws with wgpu: no snapshot of its own.
pub fn snapshot(window: &mut Window, path: &str) -> Value {
    use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
    let id = match window.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Xlib(h)) => h.window as u64,
        Ok(RawWindowHandle::Xcb(h)) => h.window.get() as u64,
        _ => return json!({ "ok": false, "error": "snapshots need X11 (Xvfb)" }),
    };
    let cmd = format!("xwd -silent -id {id} | convert xwd:- png:{path}");
    match std::process::Command::new("sh").arg("-c").arg(&cmd).status() {
        Ok(s) if s.success() => json!({ "ok": true, "path": path }),
        Ok(s) => json!({ "ok": false, "error": format!("xwd/convert failed: {s}") }),
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
}
