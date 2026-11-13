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
use slint::ComponentHandle;
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
            "currentProject": w.get_current_project().as_str(),
            "currentChat": w.get_current_chat().as_str(),
            "chatTitle": w.get_chat_title().as_str(),
            "chatState": w.get_chat_state().as_str(),
            "chatError": w.get_chat_error().as_str(),
            "draft": w.get_draft().as_str(),
            "view": w.get_view().as_str(),
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
            match name {
                "check-server" => w.invoke_check_server(arg(c, 0).into()),
                "sign-in-pair" => w.invoke_sign_in_pair(arg(c, 0).into()),
                "sign-in-password" => w.invoke_sign_in_password(arg(c, 0).into(), arg(c, 1).into(), arg(c, 2).into()),
                "sign-out" => w.invoke_sign_out(),
                "select-project" => w.invoke_select_project(arg(c, 0).into()),
                "select-chat" => w.invoke_select_chat(arg(c, 0).into()),
                "new-chat" => w.invoke_new_chat(),
                "send" => w.invoke_send(arg(c, 0).into()),
                "stop" => w.invoke_stop(),
                "respond" => w.invoke_respond(arg(c, 0).into(), arg(c, 1).into(), arg(c, 2) == "true"),
                "answer" => w.invoke_answer(arg(c, 0).into(), arg(c, 1).into()),
                "view" => w.set_view(arg(c, 0).into()),
                "draft" => w.set_draft(arg(c, 0).into()),
                other => return json!({ "ok": false, "error": format!("unknown ui action {other}") }),
            }
            json!({ "ok": true })
        }
        other => json!({ "ok": false, "error": format!("unknown cmd {other}") }),
    }
}
