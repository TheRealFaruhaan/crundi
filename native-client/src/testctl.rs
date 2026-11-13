//! A test driver, only with CRUNDI_TEST_PORT=<port>: one JSON command per
//! line on 127.0.0.1:<port>, one JSON answer per line. It runs the same
//! functions the window's buttons do, so a script can sign in, open chats and
//! send messages against a real server, and look at the result.
//!
//!   {"cmd":"state"}                         what the app holds (src/app.rs snapshot_state)
//!   {"cmd":"props"}                         what the window shows (signed-in, errors, …)
//!   {"cmd":"snapshot","path":"/tmp/x.png"}  the window as Slint drew it
//!   {"cmd":"ui","name":"send","args":["hi"]}  a window action (names below)

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
            "server": w.get_server().as_str(),
            "username": w.get_username().as_str(),
            "currentProject": w.get_current_project().as_str(),
            "projectName": w.get_project_name().as_str(),
            "currentTab": w.get_current_tab().as_str(),
            "drawerOpen": w.get_drawer_open(),
            "panes": w.get_panes().iter().map(|p| json!({ "id": p.id, "key": p.key.as_str(), "kind": p.kind.as_str(), "title": p.title.as_str(), "state": p.state.as_str(), "focused": p.focused, "x": p.x, "y": p.y, "w": p.w, "h": p.h, "mode": p.mode.as_str(), "error": p.error.as_str() })).collect::<Vec<_>>(),
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
        "ui" => {
            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let chat = || format!("live:{}", crate::app::focused_chat());
            match name {
                "check-server" => w.invoke_check_server(arg(c, 0).into()),
                "sign-in-pair" => w.invoke_sign_in_pair(arg(c, 0).into()),
                "sign-in-password" => w.invoke_sign_in_password(arg(c, 0).into(), arg(c, 1).into(), arg(c, 2).into()),
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
