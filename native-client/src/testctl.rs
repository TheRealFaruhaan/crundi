//! A test driver, only with CRUNDI_TEST_PORT=<port>: one JSON command per
//! line on 127.0.0.1:<port>, one JSON answer per line. It runs the same
//! functions the window's buttons do, so a script can sign in, open chats and
//! send messages against a real server, and look at the result.
//!
//!   {"cmd":"state"}                         what the app holds (src/app.rs snapshot_state)
//!   {"cmd":"props"}                         what the window shows (signed-in, errors, …)
//!   {"cmd":"snapshot","path":"/tmp/x.png"}  the window as Slint drew it
//!   {"cmd":"ui","name":"send","args":["hi"]}  a window action (names below)
//!   {"cmd":"mouse","kind":"move|click|dblclick|down|up|leave","x":10,"y":20}  the pointer, as a person's
//!   {"cmd":"key","text":"Escape"|"Return"|"a…"}  a key press and release (to the focused item)

use base64::Engine;
use serde_json::{json, Value};
use slint::{ComponentHandle, Model};
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::Duration,
};

pub fn start(port: u16, ui: slint::Weak<crate::MainWindow>) {
    let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) else {
        eprintln!("[native] test driver: port {port} is taken");
        return;
    };
    eprintln!("[native] test driver on 127.0.0.1:{port}");
    thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let ui = ui.clone();
            thread::spawn(move || {
                let mut out = conn.try_clone().expect("socket");
                for line in BufReader::new(conn).lines().map_while(Result::ok) {
                    let Ok(cmd) = serde_json::from_str::<Value>(&line) else { continue };
                    let (tx, rx) = mpsc::channel();
                    let ui = ui.clone();
                    let _ = slint::invoke_from_event_loop(move || { let _ = tx.send(handle(&cmd, &ui)); });
                    let answer = rx.recv_timeout(Duration::from_secs(20)).unwrap_or(json!({ "ok": false, "error": "timeout" }));
                    if writeln!(out, "{answer}").is_err() { break; }
                }
            });
        }
    });
}

/// The file the attach button's picker answers with next (the driver's
/// stand-in for the system dialog, which a test can't drive); the button's
/// own path (busy check, off-thread pick, upload) runs as for a person.
static PICK: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

/// Canned answers for the next POSTs of a kind ("send", "cancel-queued"),
/// in order: the test's stand-in for a server that refuses or is too late.
static STUBS: std::sync::Mutex<Vec<(String, Value)>> = std::sync::Mutex::new(Vec::new());

/// The next canned answer for that kind, taken once; None: the server's.
pub fn take_stub(kind: &str) -> Option<Value> {
    let mut s = STUBS.lock().ok()?;
    let i = s.iter().position(|(k, _)| k == kind)?;
    Some(s.remove(i).1)
}

/// The driver's answer for the picker, taken once; None: the real dialog.
pub fn take_pick() -> Option<std::path::PathBuf> {
    PICK.lock().ok()?.take()
}

/// The driver set an answer for the picker and it hasn't been taken.
fn pick_waiting() -> bool {
    PICK.lock().map(|p| p.is_some()).unwrap_or(false)
}

fn arg(c: &Value, i: usize) -> String {
    c.pointer(&format!("/args/{i}")).map(|v| match v { Value::String(s) => s.clone(), o => o.to_string() }).unwrap_or_default()
}

fn handle(c: &Value, ui: &slint::Weak<crate::MainWindow>) -> Value {
    let Some(w) = ui.upgrade() else { return json!({ "ok": false, "error": "no window" }) };
    match c.get("cmd").and_then(|v| v.as_str()).unwrap_or("") {
        "state" => json!({ "ok": true, "state": crate::app::snapshot_state() }),
        "props" => json!({
            "ok": true,
            "signedIn": w.get_signed_in(),
            "online": w.get_online(),
            "pickWaiting": pick_waiting(),
            "signinError": w.get_signin_error().as_str(),
            "signinBusy": w.get_signin_busy(),
            "signinChecked": w.get_signin_checked(),
            "offerPassword": w.get_offer_password(),
            "offerTelegram": w.get_offer_telegram(),
            "oauthCode": w.get_oauth_code().as_str(),
            "server": w.get_server().as_str(),
            "username": w.get_username().as_str(),
            "currentProject": w.get_current_project().as_str(),
            "projectName": w.get_project_name().as_str(),
            "currentTab": w.get_current_tab().as_str(),
            "drawerOpen": w.get_drawer_open(),
            "panes": w.get_panes().iter().map(|p| json!({ "id": p.id, "key": p.key.as_str(), "kind": p.kind.as_str(), "title": p.title.as_str(), "shortTitle": p.short_title.as_str(), "titleMax": p.title_max, "state": p.state.as_str(), "focused": p.focused, "x": p.x, "y": p.y, "w": p.w, "h": p.h, "mode": p.mode.as_str(), "modeKey": p.mode_key.as_str(), "session": p.session.as_str(), "sessionFull": p.session_full.as_str(), "model": p.model.as_str(), "cost": p.cost.as_str(), "draft": crate::app::draft_of(p.key.as_str()), "stopArmed": p.stop_armed, "qHeld": p.q_held.as_str(), "qHeldN": p.q_held_n, "qSent": p.q_sent.as_str(), "qSentKind": p.q_sent_kind.as_str(), "upOn": crate::app::upload_of(p.key.as_str()).0, "upName": crate::app::upload_of(p.key.as_str()).1, "upLabel": crate::app::upload_of(p.key.as_str()).2, "upBusy": crate::app::upload_of(p.key.as_str()).3, "planning": p.planning, "error": p.error.as_str(), "secrets": p.secrets, "secretsN": p.secrets_n, "secretsTip": p.secrets_tip.as_str(), "pinned": p.pinned, "armed": p.armed, "fs": p.fs, "narrow": p.narrow, "menu": p.menu, "drop": p.drop })).collect::<Vec<_>>(),
            "full": if w.get_full_on() { json!({ "key": w.get_full_pane().key.as_str(), "w": w.get_full_pane().w, "h": w.get_full_pane().h, "title": w.get_full_pane().title.as_str(), "state": w.get_full_pane().state.as_str() }) } else { Value::Null },
            "toast": w.get_toast().as_str(),
            "pointerModal": w.global::<crate::Pointer>().get_modal(),
            "modePicker": if w.get_mode_picker_open() { let pr = w.global::<crate::PickerProbe>(); json!({ "focus": pr.get_focus(), "focusTop": pr.get_focus_top(), "focusBottom": pr.get_focus_bottom(), "scroll": pr.get_scroll(), "viewH": pr.get_view_h(), "current": w.get_mode_current().as_str(), "start": w.get_mode_start(), "modes": w.get_mode_options().iter().map(|m| json!({ "key": m.key.as_str(), "name": m.name.as_str(), "desc": m.desc.as_str() })).collect::<Vec<_>>() }) } else { Value::Null },
            "savedDrafts": crate::app::saved_drafts(),
            "chatEnter": if w.global::<crate::Composer>().get_ctrl_enter() { "newline" } else { "enter" },
            "projectDots": w.get_projects().iter().map(|p| json!({ "alias": p.alias.as_str(), "state": p.state.as_str(), "initials": p.initials.as_str() })).collect::<Vec<_>>(),
            "dragOn": w.get_drag_on(),
            "lockedGutters": w.get_gutters().iter().filter(|g| g.locked).count(),
            "gutters": w.get_gutters().row_count(),
            "gutterViews": w.get_gutters().iter().map(|g| json!({ "split": g.split, "index": g.index, "row": g.row, "x": g.x, "y": g.y, "w": g.w, "h": g.h, "locked": g.locked })).collect::<Vec<_>>(),
        }),
        // Resize the window (logical px), as a person dragging its edge.
        "resize" => {
            let n = |k: &str| c.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            w.window().set_size(slint::LogicalSize::new(n("w"), n("h")));
            json!({ "ok": true })
        }
        "snapshot" => match w.window().take_snapshot() {
            Ok(pb) => {
                let mut png = Vec::new();
                {
                    let mut enc = png::Encoder::new(&mut png, pb.width(), pb.height());
                    enc.set_color(png::ColorType::Rgba);
                    enc.set_depth(png::BitDepth::Eight);
                    if let Ok(mut wr) = enc.write_header() { let _ = wr.write_image_data(pb.as_bytes()); }
                }
                let path = c.get("path").and_then(|p| p.as_str()).unwrap_or("");
                if path.is_empty() {
                    json!({ "ok": true, "data": base64::engine::general_purpose::STANDARD.encode(png) })
                } else {
                    match std::fs::write(path, png) { Ok(_) => json!({ "ok": true, "path": path }), Err(e) => json!({ "ok": false, "error": e.to_string() }) }
                }
            }
            Err(e) => json!({ "ok": false, "error": format!("{e:?}") }),
        },
        "mouse" => {
            use slint::platform::{PointerEventButton, WindowEvent};
            let pos = slint::LogicalPosition::new(c.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32, c.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32);
            let win = w.window();
            let click = |n: usize| for _ in 0..n {
                // As the system's press would (src/main.rs).
                crate::pointer_pressed(&w, pos.x, pos.y);
                win.dispatch_event(WindowEvent::PointerPressed { position: pos, button: PointerEventButton::Left });
                win.dispatch_event(WindowEvent::PointerReleased { position: pos, button: PointerEventButton::Left });
            };
            crate::pointer_moved(&w, pos.x, pos.y);
            match c.get("kind").and_then(|v| v.as_str()).unwrap_or("move") {
                "click" => { win.dispatch_event(WindowEvent::PointerMoved { position: pos }); click(1) }
                "dblclick" => { win.dispatch_event(WindowEvent::PointerMoved { position: pos }); click(2) }
                // A press held across calls, then moves, then its release: a drag.
                "down" => { win.dispatch_event(WindowEvent::PointerMoved { position: pos }); crate::pointer_pressed(&w, pos.x, pos.y); win.dispatch_event(WindowEvent::PointerPressed { position: pos, button: PointerEventButton::Left }) }
                "up" => { win.dispatch_event(WindowEvent::PointerMoved { position: pos }); win.dispatch_event(WindowEvent::PointerReleased { position: pos, button: PointerEventButton::Left }) }
                "leave" => win.dispatch_event(WindowEvent::PointerExited),
                _ => win.dispatch_event(WindowEvent::PointerMoved { position: pos }),
            }
            json!({ "ok": true })
        }
        "key" => {
            use slint::platform::{Key, WindowEvent};
            let t = c.get("text").and_then(|v| v.as_str()).unwrap_or("");
            // As the system's Esc would (src/main.rs): a drag is put back.
            if t == "Escape" { crate::app::drag_cancel(); }
            // Modifiers held around it ("mods": ["Control", "Shift", "Alt", "Meta"]).
            let mods: Vec<slint::SharedString> = c.get("mods").and_then(|m| m.as_array()).map(|m| m.iter().filter_map(|x| x.as_str()).map(|x| match x {
                "Control" => Key::Control.into(), "Shift" => Key::Shift.into(), "Alt" => Key::Alt.into(), _ => Key::Meta.into(),
            }).collect()).unwrap_or_default();
            for m in &mods { w.window().dispatch_event(WindowEvent::KeyPressed { text: m.clone() }); }
            let text: slint::SharedString = match t { "Escape" => Key::Escape.into(), "Return" => Key::Return.into(), "Backspace" => Key::Backspace.into(), "Tab" => Key::Tab.into(), "Backtab" => Key::Backtab.into(), "Insert" => Key::Insert.into(), "Up" => Key::UpArrow.into(), "Down" => Key::DownArrow.into(), "Home" => Key::Home.into(), "End" => Key::End.into(), o => o.into() };
            for ch in if t.len() > 1 && !["Escape", "Return", "Backspace", "Tab", "Backtab", "Insert", "Up", "Down", "Home", "End"].contains(&t) { t.chars().map(|c| c.to_string().into()).collect::<Vec<slint::SharedString>>() } else { vec![text] } {
                w.window().dispatch_event(WindowEvent::KeyPressed { text: ch.clone() });
                w.window().dispatch_event(WindowEvent::KeyReleased { text: ch });
            }
            for m in mods.iter().rev() { w.window().dispatch_event(WindowEvent::KeyReleased { text: m.clone() }); }
            json!({ "ok": true })
        }
        "ui" => {
            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let chat = || format!("live:{}", crate::app::focused_chat());
            match name {
                "check-server" => w.invoke_check_server(arg(c, 0).into()),
                "sign-in-pair" => w.invoke_sign_in_pair(arg(c, 0).into()),
                "sign-in-password" => w.invoke_sign_in_password(arg(c, 0).into(), arg(c, 1).into(), arg(c, 2).into()),
                "sign-in-browser" => w.invoke_sign_in_oauth(arg(c, 0).into(), arg(c, 1).into()),
                "sign-out" => w.invoke_sign_out(),
                "select-project" => w.invoke_select_project(arg(c, 0).into()),
                "new-chat" => crate::app::new_chat_pane(),
                "add-pane" => w.invoke_add_pane(arg(c, 0).into()),
                "close-pane" => w.invoke_close_pane(arg(c, 0).parse().unwrap_or(0)),
                "focus-pane" => w.invoke_focus_pane(arg(c, 0).parse().unwrap_or(0)),
                "split" => w.invoke_split(arg(c, 0).parse().unwrap_or(0), arg(c, 1) == "true"),
                "launch-chat" => w.invoke_launch_chat(arg(c, 0).parse().unwrap_or(0), arg(c, 1) == "skip"),
                "send" => w.invoke_send(chat().into(), arg(c, 0).into()),
                "stop" => w.invoke_stop(chat().into()),
                // The composer (2.5): the Enter mode, the focus in a chat's box.
                "chat-enter" => w.global::<crate::Composer>().invoke_set_ctrl_enter(arg(c, 0) == "newline"),
                "composer-focus" => crate::app::focus_composer(&chat()),
                // The permission-mode picker (2.6): open it on the focused
                // chat's button, choose, close.
                "mode-open" => w.global::<crate::Composer>().invoke_mode(chat().into()),
                "mode-choose" => w.invoke_mode_choose(arg(c, 0).into()),
                "mode-close" => w.invoke_mode_close(),
                // Attachments (2.7), on the focused chat: a file as if picked
                // in the dialog; files as if dropped from the system (args after
                // the first; "upload" first: as on a remote server); a picture
                // or a file as if pasted; the caret (and selection) in the box.
                "attach" => crate::app::upload_file(&chat(), std::path::Path::new(&arg(c, 0))),
                "drop" => {
                    let all: Vec<String> = (0..16).map(|i| arg(c, i)).filter(|x| !x.is_empty()).collect();
                    let force = all.first().is_some_and(|x| x == "upload");
                    crate::app::drop_files(&chat(), all.into_iter().filter(|x| x != "upload").map(std::path::PathBuf::from).collect(), force);
                }
                // The attach button as clicked, its picker answered by
                // "pick-next" (empty: no answer, the real dialog).
                // Queued messages (2.9): a canned answer for the next POST of a
                // kind ("send", "cancel-queued"), an event played into the
                // focused chat, a take-back as a click on a drawer.
                "stub" => {
                    let v: Value = serde_json::from_str(&arg(c, 1)).unwrap_or(Value::Null);
                    if let Ok(mut s) = STUBS.lock() { s.push((arg(c, 0), v)); }
                }
                "chat-event" => crate::app::play_event(crate::app::focused_chat().as_str(), serde_json::from_str(&arg(c, 0)).unwrap_or(Value::Null)),
                "take-back" => { crate::app::take_back(&chat()); }
                "pick-next" => { let a = arg(c, 0); if let Ok(mut p) = PICK.lock() { *p = (!a.is_empty()).then(|| a.into()); } }
                "attach-button" => w.global::<crate::Composer>().invoke_attach(chat().into()),
                // Files dropped as the system delivers them (src/main.rs: one
                // DroppedFile each, the same moment), on the chat the drag is
                // over ("file-hover" true and the pointer over it) or else the
                // focused chat.
                "os-drop" => { for i in 0..16 { let a = arg(c, i); if !a.is_empty() { crate::app::file_dropped(a.into()); } } }
                "paste-image" => match std::fs::read(arg(c, 0)) { Ok(b) => crate::app::upload(&chat(), "image.png".into(), b), Err(e) => return json!({ "ok": false, "error": e.to_string() }) },
                // Files dragged from the system over the window (true) or gone
                // (false): with the pointer over a chat, its drop outline.
                "file-hover" => crate::app::file_hover(arg(c, 0) == "true"),
                // Text on the system's clipboard (for Shift+Insert).
                "clipboard-text" => { if let Ok(mut cb) = arboard::Clipboard::new() { let _ = cb.set_text(arg(c, 0)); crate::app::keep_clipboard(cb); } }
                "paste-file" => crate::app::upload_file(&chat(), std::path::Path::new(&arg(c, 0))),
                "caret" => { let at: usize = arg(c, 0).parse().unwrap_or(0); crate::app::set_caret(&chat(), at, arg(c, 1).parse().unwrap_or(at)); }
                "respond" => w.invoke_respond(chat().into(), arg(c, 0).into(), arg(c, 1).into(), arg(c, 2) == "true"),
                "answer" => w.invoke_answer(chat().into(), arg(c, 0).into(), arg(c, 1).into()),
                // The pane header, on a leaf id (as the "panes" list gives them).
                "pane-close" => w.invoke_pane_close(arg(c, 0).parse().unwrap_or(0)),
                "pane-full" => w.invoke_pane_full(arg(c, 0).parse().unwrap_or(0)),
                "pane-font" => w.invoke_pane_font(arg(c, 0).parse().unwrap_or(0), arg(c, 1).parse().unwrap_or(0)),
                "pane-pin" => w.invoke_pane_pin(arg(c, 0).parse().unwrap_or(0)),
                "pane-rename" => w.invoke_pane_rename(arg(c, 0).parse().unwrap_or(0), arg(c, 1).into()),
                "pane-maximize" => w.invoke_pane_maximize(arg(c, 0).parse().unwrap_or(0)),
                "pane-menu" => w.invoke_pane_menu(arg(c, 0).parse().unwrap_or(0), arg(c, 1) != "false"),
                "pane-drag" => w.invoke_pane_drag(arg(c, 0).parse().unwrap_or(0), arg(c, 1).parse().unwrap_or(0), arg(c, 2).parse().unwrap_or(0.0), arg(c, 3).parse().unwrap_or(0.0)),
                "chip" => w.invoke_chip(arg(c, 0).parse().unwrap_or(0), arg(c, 1).into()),
                "pane-secrets" => w.invoke_pane_secrets(arg(c, 0).parse().unwrap_or(0)),
                "tab" => w.set_current_tab(arg(c, 0).into()),
                "drawer" => w.set_drawer_open(arg(c, 0) == "true"),
                "add-menu" => w.set_add_menu_open(arg(c, 0) == "true"),
                other => return json!({ "ok": false, "error": format!("unknown ui action {other}") }),
            }
            json!({ "ok": true })
        }
        other => json!({ "ok": false, "error": format!("unknown cmd {other}") }),
    }
}
