//! A test driver, only with CRUNDI_TEST_PORT=<port>: one JSON command per
//! line on 127.0.0.1:<port>, one JSON answer per line. It runs the same
//! functions the window's buttons do, so a script can sign in, open chats and
//! send messages against a real server, and look at the result.
//!
//!   {"cmd":"state"}                         what the app holds (src/app.rs snapshot_state)
//!   {"cmd":"props"}                         what the window shows (signed-in, errors, …)
//!   {"cmd":"snapshot","path":"/tmp/x.png"}  the window as Slint drew it
//!   {"cmd":"ui","name":"send","args":["hi"]}  a window action (names below)
//!   {"cmd":"mouse","kind":"move|click|dblclick|leave","x":10,"y":20}  the pointer, as a person's
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
            "panes": w.get_panes().iter().map(|p| json!({ "id": p.id, "key": p.key.as_str(), "kind": p.kind.as_str(), "title": p.title.as_str(), "shortTitle": p.short_title.as_str(), "titleMax": p.title_max, "state": p.state.as_str(), "focused": p.focused, "x": p.x, "y": p.y, "w": p.w, "h": p.h, "mode": p.mode.as_str(), "error": p.error.as_str(), "secrets": p.secrets, "secretsN": p.secrets_n, "secretsTip": p.secrets_tip.as_str(), "pinned": p.pinned, "armed": p.armed, "fs": p.fs })).collect::<Vec<_>>(),
            "full": if w.get_full_on() { json!({ "key": w.get_full_pane().key.as_str(), "w": w.get_full_pane().w, "h": w.get_full_pane().h }) } else { Value::Null },
            "toast": w.get_toast().as_str(),
            "lockedGutters": w.get_gutters().iter().filter(|g| g.locked).count(),
            "gutters": w.get_gutters().row_count(),
        }),
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
                win.dispatch_event(WindowEvent::PointerPressed { position: pos, button: PointerEventButton::Left });
                win.dispatch_event(WindowEvent::PointerReleased { position: pos, button: PointerEventButton::Left });
            };
            match c.get("kind").and_then(|v| v.as_str()).unwrap_or("move") {
                "click" => { win.dispatch_event(WindowEvent::PointerMoved { position: pos }); click(1) }
                "dblclick" => { win.dispatch_event(WindowEvent::PointerMoved { position: pos }); click(2) }
                "leave" => win.dispatch_event(WindowEvent::PointerExited),
                _ => win.dispatch_event(WindowEvent::PointerMoved { position: pos }),
            }
            json!({ "ok": true })
        }
        "key" => {
            use slint::platform::{Key, WindowEvent};
            let t = c.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let text: slint::SharedString = match t { "Escape" => Key::Escape.into(), "Return" => Key::Return.into(), "Backspace" => Key::Backspace.into(), o => o.into() };
            for ch in if t.len() > 1 && !["Escape", "Return", "Backspace"].contains(&t) { t.chars().map(|c| c.to_string().into()).collect::<Vec<slint::SharedString>>() } else { vec![text] } {
                w.window().dispatch_event(WindowEvent::KeyPressed { text: ch.clone() });
                w.window().dispatch_event(WindowEvent::KeyReleased { text: ch });
            }
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
                "launch-chat" => w.invoke_launch_chat(arg(c, 0).parse().unwrap_or(0), false),
                "send" => w.invoke_send(chat().into(), arg(c, 0).into()),
                "stop" => w.invoke_stop(chat().into()),
                "respond" => w.invoke_respond(chat().into(), arg(c, 0).into(), arg(c, 1).into(), arg(c, 2) == "true"),
                "answer" => w.invoke_answer(chat().into(), arg(c, 0).into(), arg(c, 1).into()),
                // The pane header, on a leaf id (as the "panes" list gives them).
                "pane-close" => w.invoke_pane_close(arg(c, 0).parse().unwrap_or(0)),
                "pane-full" => w.invoke_pane_full(arg(c, 0).parse().unwrap_or(0)),
                "pane-font" => w.invoke_pane_font(arg(c, 0).parse().unwrap_or(0), arg(c, 1).parse().unwrap_or(0)),
                "pane-pin" => w.invoke_pane_pin(arg(c, 0).parse().unwrap_or(0)),
                "pane-rename" => w.invoke_pane_rename(arg(c, 0).parse().unwrap_or(0), arg(c, 1).into()),
                "pane-maximize" => w.invoke_pane_maximize(arg(c, 0).parse().unwrap_or(0)),
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
