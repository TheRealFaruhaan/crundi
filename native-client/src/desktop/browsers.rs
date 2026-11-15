//! The Browsers tab (src/webapp-html.js loadBrowsers, renderBrowsers): the
//! browsers chats opened (GET /api/browsers), opened, driven and closed from
//! here. Loaded when the tab comes on show, again 500 ms after an open or a
//! close; no live updates (as the web).

use crate::{kit, DesktopBrowserCard, DesktopBrowsers};
use serde_json::{json, Value};
use slint::{ComponentHandle, SharedString, VecModel};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

/// The web's own device list for this tab (DEVICE_PROFILES): name, w, h, group.
pub const PROFILES: &[(&str, i32, i32, &str)] = &[
    ("Desktop 1280\u{d7}720", 1280, 720, "Desktop"), ("Full HD 1920\u{d7}1080", 1920, 1080, "Desktop"),
    ("iPhone SE", 375, 667, "Apple"), ("iPhone X", 375, 812, "Apple"), ("iPhone 12 Pro", 390, 844, "Apple"), ("iPhone 14 Pro", 393, 852, "Apple"),
    ("iPhone 15 Pro", 393, 852, "Apple"), ("iPhone 16 Pro", 393, 852, "Apple"), ("iPhone 16 Pro Max", 430, 932, "Apple"), ("iPad Air", 820, 1180, "Apple"), ("iPad Pro", 1024, 1366, "Apple"),
    ("Pixel 7", 412, 915, "Google"), ("Pixel 9 Pro", 430, 932, "Google"),
    ("Galaxy S24 Ultra", 412, 915, "Samsung"), ("Galaxy Z Fold 5", 344, 882, "Samsung"),
    ("Surface Pro 9", 1200, 1800, "Microsoft"),
];

#[derive(Default)]
struct St { list: Vec<Value>, shots: HashMap<String, (String, slint::Image, String)>, loaded: bool }
thread_local! { static S: RefCell<St> = RefCell::new(St::default()); }

/// "iPhone 16 Pro (393×852)", grouped as the web's optgroups.
pub fn labels() -> Vec<String> {
    PROFILES.iter().map(|(n, w, h, g)| format!("{g} · {n} ({w}\u{d7}{h})")).collect()
}

pub fn load() {
    let alias = kit::project();
    kit::call(move |api| api.get(&format!("/api/browsers?alias={}", kit::enc(&alias))), |r| match r {
        Ok(v) => {
            let places = v.get("places").cloned().unwrap_or(Value::Null);
            S.with(|s| { let mut s = s.borrow_mut(); s.list = v.get("browsers").and_then(|b| b.as_array()).cloned().unwrap_or_default(); s.loaded = true; });
            if let Some(w) = kit::ui() { w.global::<DesktopBrowsers>().set_places(kit::truthy(&places["server"]) && kit::truthy(&places["desktop"])); }
            render();
        }
        Err(e) => kit::toast_error(&format!("Failed to load browsers: {e}")),
    });
}

fn later() { slint::Timer::single_shot(Duration::from_millis(500), load); }

pub fn open() {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<DesktopBrowsers>();
    let url = g.get_url().trim().to_string();
    if url.is_empty() { return kit::toast_error("Enter a URL first"); }
    let (name, pw, ph, _) = PROFILES[(g.get_device().max(0) as usize).min(PROFILES.len() - 1)];
    let on = match g.get_place() { 1 => json!("server"), 2 => json!("desktop"), _ => Value::Null };
    let mut body = json!({ "alias": kit::project(), "name": format!("browser-{}", crate::browser::now_ms()), "url": url, "width": pw, "height": ph });
    if !on.is_null() { body["on"] = on; }
    kit::call(move |api| api.post("/api/browsers/open", &body), move |r| {
        match r {
            Ok(v) if kit::truthy(&v["ok"]) => kit::toast_ok(&format!("Browser opened ({name})")),
            Ok(v) => kit::toast_error(&Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Failed to open browser".into())),
            Err(e) => kit::toast_error(&format!("Error: {e}")),
        }
        later();
    });
}

pub fn go(key: &str, url: &str) {
    let mut url = url.trim().to_string();
    if url.is_empty() { return; }
    let l = url.to_lowercase();
    if !l.starts_with("http://") && !l.starts_with("https://") && !l.starts_with("about:") {
        url = format!("{}{url}", if l.starts_with("localhost") { "http://" } else { "https://" });
    }
    let path = format!("/api/browsers/{}/navigate", kit::enc(key));
    kit::call(move |api| api.post(&path, &json!({ "url": url })), |r| match r {
        Ok(v) if !kit::truthy(&v["ok"]) => kit::toast_error(&Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Navigate failed".into())),
        Ok(_) => {}
        Err(e) => kit::toast_error(&format!("Error: {e}")),
    });
}

pub fn shot(key: &str) {
    let k = key.to_string();
    S.with(|s| { s.borrow_mut().shots.insert(k.clone(), ("loading".into(), slint::Image::default(), String::new())); });
    render();
    let path = format!("/api/browsers/{}/screenshot", kit::enc(key));
    kit::call(move |api| api.get(&path), move |r| {
        let st = match r {
            Ok(v) if kit::truthy(&v["ok"]) && !kit::s(&v, "data").is_empty() => {
                use base64::Engine;
                match base64::engine::general_purpose::STANDARD.decode(kit::s(&v, "data")).ok().and_then(|b| image::load_from_memory(&b).ok()) {
                    Some(i) => { let i = i.to_rgba8(); ("ok".to_string(), slint::Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(i.as_raw(), i.width(), i.height())), String::new()) }
                    None => ("error".into(), slint::Image::default(), "Screenshot failed".into()),
                }
            }
            Ok(v) => ("error".into(), slint::Image::default(), Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Screenshot failed".into())),
            Err(e) => ("error".into(), slint::Image::default(), format!("Error: {e}")),
        };
        S.with(|s| { s.borrow_mut().shots.insert(k, st); });
        render();
    });
}

pub fn close(key: &str) {
    let path = format!("/api/browsers/{}/close", kit::enc(key));
    kit::call(move |api| api.post(&path, &json!({})), |r| {
        match r {
            Ok(v) if kit::truthy(&v["ok"]) => kit::toast_ok("Browser closed"),
            Ok(v) => kit::toast_error(&Some(kit::s(&v, "error")).filter(|e| !e.is_empty()).unwrap_or_else(|| "Failed".into())),
            Err(e) => kit::toast_error(&format!("Error: {e}")),
        }
        later();
    });
}

pub fn render() {
    let Some(w) = kit::ui() else { return };
    let g = w.global::<DesktopBrowsers>();
    let (list, shots, loaded) = S.with(|s| { let s = s.borrow(); (s.list.clone(), s.shots.clone(), s.loaded) });
    let cards: Vec<DesktopBrowserCard> = list.iter().map(|b| {
        let key = kit::s(b, "key");
        let (st, img, err) = shots.get(&key).cloned().unwrap_or_default();
        DesktopBrowserCard {
            name: Some(kit::s(b, "name")).filter(|x| !x.is_empty()).unwrap_or_else(|| if key.is_empty() { "browser".into() } else { key.clone() }).into(),
            place: match kit::s(b, "on").as_str() { "server" => "on the server", "desktop" => "on the desktop app", _ => "open" }.into(),
            url: kit::s(b, "url").into(), shot: img, shot_state: st.into(), shot_error: err.into(), key: key.into(),
        }
    }).collect();
    g.set_cards(Rc::new(VecModel::from(cards)).into());
    g.set_loaded(loaded);
}

pub fn init(w: &crate::MainWindow) {
    let g = w.global::<DesktopBrowsers>();
    g.set_devices(Rc::new(VecModel::from(labels().into_iter().map(SharedString::from).collect::<Vec<_>>())).into());
    g.set_device(PROFILES.iter().position(|p| p.0 == "iPhone 16 Pro").unwrap_or(0) as i32);
    g.on_open(open);
    g.on_go(|k, u| go(&k, &u));
    g.on_shot(|k| shot(&k));
    g.on_close(|k| close(&k));
}

pub fn props() -> Value {
    S.with(|s| { let s = s.borrow(); json!({ "loaded": s.loaded, "list": s.list.iter().map(|b| json!({ "key": b["key"], "url": b["url"], "on": b["on"] })).collect::<Vec<_>>(), "shots": s.shots.iter().map(|(k, v)| (k.clone(), json!(v.0))).collect::<serde_json::Map<_, _>>() }) })
}
