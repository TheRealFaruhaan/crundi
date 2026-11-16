//! The test driver's commands, answered on the UI thread (the protocol is
//! crundi_core::driver's, the Slint client's testctl): the window's own
//! actions and props here, every part's through the registry (unprefixed
//! names are offered to each part in turn, "<area>:<name>" to that part).

use crundi_core::driver::Request;
use crundi_shell::{registry, session};
use gpui::*;
use serde_json::{json, Map, Value};

pub fn handle(r: Request, window: &mut Window, cx: &mut App) {
    let answer = match r.name() {
        "state" => json!({ "ok": true, "state": Value::Object(registry::merged(window, cx, true)) }),
        "props" => json!(props(window, cx)),
        "area" => json!({ "ok": true, "props": registry::props_of(&r.str("area"), window, cx) }),
        // With a path: the PNG written there; without: the PNG as base64 `data` (phase2's `shot`).
        "snapshot" => {
            let path = r.str("path");
            if path.is_empty() {
                let tmp = std::env::temp_dir().join(format!("crundi-gpui-shot-{}.png", std::process::id()));
                let v = snapshot(window, &tmp.to_string_lossy());
                match std::fs::read(&tmp) {
                    Ok(b) if v.get("ok") == Some(&json!(true)) => { let _ = std::fs::remove_file(&tmp); use base64::Engine as _; json!({ "ok": true, "data": base64::engine::general_purpose::STANDARD.encode(b) }) }
                    _ => v,
                }
            } else { snapshot(window, &path) }
        }
        "resize" => { window.resize(size(px(r.num("w") as f32), px(r.num("h") as f32))); json!({ "ok": true }) }
        "mouse" => { mouse(&r, window, cx); json!({ "ok": true }) }
        "key" => { key(&r, window, cx); json!({ "ok": true }) }
        "ui" => ui(&r, window, cx),
        other => json!({ "ok": false, "error": format!("unknown cmd {other}") }),
    };
    window.refresh();
    r.answer(answer);
}

fn props(window: &mut Window, cx: &mut App) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("ok".into(), json!(true));
    if let Some(Value::Object(s)) = crundi_shell::shell(cx).map(|s| s.read(cx).props(cx)) { m.extend(s); }
    m.insert("pickWaiting".into(), json!(crundi_core::driver::pick_waiting()));
    m.insert("dialogOpen".into(), json!(crundi_widgets::dialog_open(window, cx)));
    // The ask layer's open confirm / prompt and context menu (the Slint client's "dialog", "menu").
    m.insert("dialog".into(), crundi_widgets::ask_props(cx));
    m.insert("menu".into(), crundi_widgets::menu_props(cx));
    registry::deep_merge(&mut m, registry::merged(window, cx, false));
    // Each part's props under areas.<part> too (the area tests read them
    // there; "ops" is Operations' name in the tests), unless the part put
    // its own there (F41).
    let mut areas = serde_json::Map::new();
    // pointerModal: any part's dialog over the window, the shared view modal
    // or the ask layer (F58): computed here, so no part's false hides another's true.
    let mut modal = crundi_widgets::modal_open(cx) || !crundi_widgets::ask_props(cx).is_null();
    for name in registry::names(cx) {
        let v = registry::props_of(name, window, cx);
        if v.is_null() { continue; }
        modal |= v.get("pointerModal") == Some(&json!(true));
        if name == "operations" { areas.insert("ops".into(), v.clone()); }
        areas.insert(name.to_string(), v);
    }
    let have = m.remove("areas").and_then(|a| if let Value::Object(o) = a { Some(o) } else { None }).unwrap_or_default();
    registry::deep_merge(&mut areas, have);
    m.insert("areas".into(), Value::Object(areas));
    m.insert("pointerModal".into(), json!(modal));
    // The toast on show: the widgets crate's one record of it (after the
    // parts' props, so no part's own copy hides it).
    m.insert("toast".into(), json!(crundi_widgets::toast_text(cx).to_string()));
    m
}

fn ui(r: &Request, window: &mut Window, cx: &mut App) -> Value {
    let name = r.str("name");
    let a = |i| r.arg(i);
    match name.as_str() {
        "check-server" => session::check_server(cx, &a(0)),
        "sign-in-password" => session::sign_in_password(cx, &a(0), &a(1), &a(2)),
        "sign-in-pair" => session::sign_in_pair(cx, &a(0)),
        "sign-in-browser" => session::sign_in_oauth(cx, &a(0), &a(1)),
        "cancel-sign-in" => session::cancel_browser(cx),
        "sign-out" => session::sign_out(cx, ""),
        "select-project" => session::select_project(cx, &a(0)),
        "tab" => { if let Some(s) = crundi_shell::shell(cx) { let t = a(0); s.update(cx, |s, cx| s.show_tab(&t, cx)); } }
        "drawer" => crundi_shell::set_drawer(cx, a(0) == "true"),
        "pick-next" => { let p = a(0); crundi_core::driver::set_pick((!p.is_empty()).then(|| p.into())); }
        "stub" => crundi_core::driver::push_stub(&a(0), serde_json::from_str(&a(1)).unwrap_or(Value::Null)),
        // The ask layer's confirm or prompt (OK / Cancel, the prompt's text), else the dialog on top closes.
        "dialog-answer" => { let v = r.args().get(1).cloned(); if !crundi_widgets::ask_answer(a(0) == "true", v, window, cx) { crundi_widgets::close_dialog(window, cx) } }
        "menu-pick" => { crundi_widgets::menu_pick(&a(0), window, cx); }
        // Take the toasts down (a screen shot without an earlier step's toast over it).
        "toasts-clear" => crundi_widgets::clear_toasts(window, cx),
        _ => {
            return registry::test_ui(&name, &r.args(), window, cx)
                .unwrap_or_else(|| json!({ "ok": false, "error": format!("unknown ui action {name}") }));
        }
    }
    json!({ "ok": true })
}

fn point_of(r: &Request) -> Point<Pixels> { point(px(r.num("x") as f32), px(r.num("y") as f32)) }

/// The pointer, as a person's: move, click, dblclick, down, up, leave, wheel (dy).
fn mouse(r: &Request, window: &mut Window, cx: &mut App) {
    let p = point_of(r);
    let m = Modifiers::default();
    let mv = |w: &mut Window, cx: &mut App, b: Option<MouseButton>| { w.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position: p, pressed_button: b, modifiers: m }), cx); };
    let down = |w: &mut Window, cx: &mut App, n: usize| { w.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: p, modifiers: m, click_count: n, first_mouse: false }), cx); };
    let up = |w: &mut Window, cx: &mut App, n: usize| { w.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: p, modifiers: m, click_count: n }), cx); };
    match r.str("kind").as_str() {
        "click" => { mv(window, cx, None); down(window, cx, 1); up(window, cx, 1); }
        "dblclick" => { mv(window, cx, None); down(window, cx, 1); up(window, cx, 1); down(window, cx, 2); up(window, cx, 2); }
        "down" => { mv(window, cx, None); down(window, cx, 1); }
        "up" => { mv(window, cx, Some(MouseButton::Left)); up(window, cx, 1); }
        "leave" => { window.dispatch_event(PlatformInput::MouseExited(MouseExitEvent { position: p, pressed_button: None, modifiers: m }), cx); }
        "wheel" => {
            mv(window, cx, None);
            let dy = r.num("dy") as f32;
            window.dispatch_event(PlatformInput::Scroll(ScrollEvent { position: p, delta: ScrollDelta::Pixels(point(px(0.), px(dy))), modifiers: m, touch_phase: TouchPhase::Moved, source: ScrollSource::Pointer }), cx);
        }
        _ => mv(window, cx, None),
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
    let press = |key: String, ch: Option<String>, modifiers: Modifiers, window: &mut Window, cx: &mut App| {
        let ks = Keystroke { modifiers, key, key_char: ch };
        window.dispatch_keystroke(ks.clone(), cx);
        window.dispatch_event(PlatformInput::KeyUp(KeyUpEvent { keystroke: ks }), cx);
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
/// convert). The GPUI client has no snapshot of its own (it draws with wgpu).
fn snapshot(window: &mut Window, path: &str) -> Value {
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
